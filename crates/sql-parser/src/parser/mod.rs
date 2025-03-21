use errors::{SqlParseError, SqlRequired, SqlUnsupported};
use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, Ident, Join, JoinConstraint, JoinOperator,
    ObjectName, Query, SelectItem, TableAlias, TableFactor, TableWithJoins, UnaryOperator, Value,
    WildcardAdditionalOptions,
};

use crate::ast::{
    BinOp, LogOp, Parameter, Project, ProjectElem, ProjectExpr, SqlExpr, SqlFrom, SqlIdent, SqlJoin, SqlLiteral,
};

pub mod errors;
pub mod sql;
pub mod sub;

pub type SqlParseResult<T> = core::result::Result<T, SqlParseError>;

/// Methods for parsing a relation expression.
/// Note we abstract over the type of the relation expression,
/// as each language has a different definition for it.
trait RelParser {
    type Ast;

    /// Parse a top level relation expression
    fn parse_query(query: Query) -> SqlParseResult<Self::Ast>;

    /// Parse a FROM clause
    fn parse_from(mut tables: Vec<TableWithJoins>) -> SqlParseResult<SqlFrom> {
        if tables.is_empty() {
            return Err(SqlRequired::From.into());
        }
        if tables.len() > 1 {
            return Err(SqlUnsupported::ImplicitJoins.into());
        }
        let TableWithJoins { relation, joins } = tables.swap_remove(0);
        let (name, alias) = Self::parse_relvar(relation)?;
        if joins.is_empty() {
            return Ok(SqlFrom::Expr(name, alias));
        }
        Ok(SqlFrom::Join(name, alias, Self::parse_joins(joins)?))
    }

    /// Parse a sequence of JOIN clauses
    fn parse_joins(joins: Vec<Join>) -> SqlParseResult<Vec<SqlJoin>> {
        joins.into_iter().map(Self::parse_join).collect()
    }

    /// Parse a single JOIN clause
    fn parse_join(join: Join) -> SqlParseResult<SqlJoin> {
        let (var, alias) = Self::parse_relvar(join.relation)?;
        match join.join_operator {
            JoinOperator::CrossJoin => Ok(SqlJoin { var, alias, on: None }),
            JoinOperator::Inner(JoinConstraint::None) => Ok(SqlJoin { var, alias, on: None }),
            JoinOperator::Inner(JoinConstraint::On(Expr::BinaryOp {
                left,
                op: BinaryOperator::Eq,
                right,
            })) if matches!(*left, Expr::Identifier(..) | Expr::CompoundIdentifier(..))
                && matches!(*right, Expr::Identifier(..) | Expr::CompoundIdentifier(..)) =>
            {
                Ok(SqlJoin {
                    var,
                    alias,
                    on: Some(parse_expr(Expr::BinaryOp {
                        left,
                        op: BinaryOperator::Eq,
                        right,
                    })?),
                })
            }
            _ => Err(SqlUnsupported::JoinType.into()),
        }
    }

    /// Parse a table reference in a FROM clause
    fn parse_relvar(expr: TableFactor) -> SqlParseResult<(SqlIdent, SqlIdent)> {
        match expr {
            // Relvar no alias
            TableFactor::Table {
                name,
                alias: None,
                args: None,
                with_hints,
                version: None,
                partitions,
            } if with_hints.is_empty() && partitions.is_empty() => {
                let name = parse_ident(name)?;
                let alias = name.clone();
                Ok((name, alias))
            }
            // Relvar with alias
            TableFactor::Table {
                name,
                alias: Some(TableAlias { name: alias, columns }),
                args: None,
                with_hints,
                version: None,
                partitions,
            } if with_hints.is_empty() && partitions.is_empty() && columns.is_empty() => {
                Ok((parse_ident(name)?, alias.into()))
            }
            _ => Err(SqlUnsupported::From(expr).into()),
        }
    }
}

/// Parse the items of a SELECT clause
pub(crate) fn parse_projection(mut items: Vec<SelectItem>) -> SqlParseResult<Project> {
    if items.len() == 1 {
        return parse_project_or_agg(items.swap_remove(0));
    }
    Ok(Project::Exprs(
        items
            .into_iter()
            .map(parse_project_elem)
            .collect::<SqlParseResult<_>>()?,
    ))
}

/// Parse a SELECT clause with only a single item
pub(crate) fn parse_project_or_agg(item: SelectItem) -> SqlParseResult<Project> {
    match item {
        SelectItem::Wildcard(WildcardAdditionalOptions {
            opt_exclude: None,
            opt_except: None,
            opt_rename: None,
            opt_replace: None,
        }) => Ok(Project::Star(None)),
        SelectItem::QualifiedWildcard(
            table_name,
            WildcardAdditionalOptions {
                opt_exclude: None,
                opt_except: None,
                opt_rename: None,
                opt_replace: None,
            },
        ) => Ok(Project::Star(Some(parse_ident(table_name)?))),
        SelectItem::UnnamedExpr(Expr::Function(_)) => Err(SqlUnsupported::AggregateWithoutAlias.into()),
        SelectItem::ExprWithAlias {
            expr: Expr::Function(agg_fn),
            alias,
        } => parse_agg_fn(agg_fn, alias.into()),
        SelectItem::UnnamedExpr(_) | SelectItem::ExprWithAlias { .. } => {
            Ok(Project::Exprs(vec![parse_project_elem(item)?]))
        }
        item => Err(SqlUnsupported::Projection(item).into()),
    }
}

/// Parse an aggregate function in a select list
fn parse_agg_fn(agg_fn: Function, alias: SqlIdent) -> SqlParseResult<Project> {
    fn is_count(name: &ObjectName) -> bool {
        name.0.len() == 1
            && name
                .0
                .first()
                .is_some_and(|Ident { value, .. }| value.to_lowercase() == "count")
    }
    match agg_fn {
        Function {
            name,
            args,
            over: None,
            distinct: false,
            special: false,
            order_by,
        } if is_count(&name)
            && order_by.is_empty()
            && args.len() == 1
            && args
                .first()
                .is_some_and(|arg| matches!(arg, FunctionArg::Unnamed(FunctionArgExpr::Wildcard))) =>
        {
            Ok(Project::Count(alias))
        }
        agg_fn => Err(SqlUnsupported::Aggregate(agg_fn).into()),
    }
}

/// Parse an item in a SELECT clause
pub(crate) fn parse_project_elem(item: SelectItem) -> SqlParseResult<ProjectElem> {
    match item {
        SelectItem::Wildcard(_) => Err(SqlUnsupported::MixedWildcardProject.into()),
        SelectItem::QualifiedWildcard(..) => Err(SqlUnsupported::MixedWildcardProject.into()),
        SelectItem::UnnamedExpr(expr) => match parse_proj(expr)? {
            ProjectExpr::Var(name) => Ok(ProjectElem(ProjectExpr::Var(name.clone()), name)),
            ProjectExpr::Field(name, field) => Ok(ProjectElem(ProjectExpr::Field(name, field.clone()), field)),
        },
        SelectItem::ExprWithAlias { expr, alias } => Ok(ProjectElem(parse_proj(expr)?, alias.into())),
    }
}

/// Parse a column projection
pub(crate) fn parse_proj(expr: Expr) -> SqlParseResult<ProjectExpr> {
    match expr {
        Expr::Identifier(ident) => Ok(ProjectExpr::Var(ident.into())),
        Expr::CompoundIdentifier(mut idents) if idents.len() == 2 => {
            let table = idents.swap_remove(0).into();
            let field = idents.swap_remove(0).into();
            Ok(ProjectExpr::Field(table, field))
        }
        _ => Err(SqlUnsupported::ProjectionExpr(expr).into()),
    }
}

/// Parse a scalar expression
pub(crate) fn parse_expr(expr: Expr) -> SqlParseResult<SqlExpr> {
    fn signed_num(sign: impl Into<String>, expr: Expr) -> Result<SqlExpr, SqlUnsupported> {
        match expr {
            Expr::Value(Value::Number(n, _)) => Ok(SqlExpr::Lit(SqlLiteral::Num((sign.into() + &n).into_boxed_str()))),
            expr => Err(SqlUnsupported::Expr(expr)),
        }
    }
    match expr {
        Expr::Nested(expr) => parse_expr(*expr),
        Expr::Value(Value::Placeholder(param)) if &param == ":sender" => Ok(SqlExpr::Param(Parameter::Sender)),
        Expr::Value(v) => Ok(SqlExpr::Lit(parse_literal(v)?)),
        Expr::UnaryOp {
            op: UnaryOperator::Plus,
            expr,
        } if matches!(&*expr, Expr::Value(Value::Number(..))) => {
            signed_num("+", *expr).map_err(SqlParseError::SqlUnsupported)
        }
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr,
        } if matches!(&*expr, Expr::Value(Value::Number(..))) => {
            signed_num("-", *expr).map_err(SqlParseError::SqlUnsupported)
        }
        Expr::Identifier(ident) => Ok(SqlExpr::Var(ident.into())),
        Expr::CompoundIdentifier(mut idents) if idents.len() == 2 => {
            let table = idents.swap_remove(0).into();
            let field = idents.swap_remove(0).into();
            Ok(SqlExpr::Field(table, field))
        }
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => {
            let l = parse_expr(*left)?;
            let r = parse_expr(*right)?;
            Ok(SqlExpr::Log(Box::new(l), Box::new(r), LogOp::And))
        }
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Or,
            right,
        } => {
            let l = parse_expr(*left)?;
            let r = parse_expr(*right)?;
            Ok(SqlExpr::Log(Box::new(l), Box::new(r), LogOp::Or))
        }
        Expr::BinaryOp { left, op, right } => {
            let l = parse_expr(*left)?;
            let r = parse_expr(*right)?;
            Ok(SqlExpr::Bin(Box::new(l), Box::new(r), parse_binop(op)?))
        }
        _ => Err(SqlUnsupported::Expr(expr).into()),
    }
}

/// Parse an optional scalar expression
pub(crate) fn parse_expr_opt(opt: Option<Expr>) -> SqlParseResult<Option<SqlExpr>> {
    opt.map(parse_expr).transpose()
}

/// Parse a scalar binary operator
pub(crate) fn parse_binop(op: BinaryOperator) -> SqlParseResult<BinOp> {
    match op {
        BinaryOperator::Eq => Ok(BinOp::Eq),
        BinaryOperator::NotEq => Ok(BinOp::Ne),
        BinaryOperator::Lt => Ok(BinOp::Lt),
        BinaryOperator::LtEq => Ok(BinOp::Lte),
        BinaryOperator::Gt => Ok(BinOp::Gt),
        BinaryOperator::GtEq => Ok(BinOp::Gte),
        _ => Err(SqlUnsupported::BinOp(op).into()),
    }
}

/// Parse a literal expression
pub(crate) fn parse_literal(value: Value) -> SqlParseResult<SqlLiteral> {
    match value {
        Value::Boolean(v) => Ok(SqlLiteral::Bool(v)),
        Value::Number(v, _) => Ok(SqlLiteral::Num(v.into_boxed_str())),
        Value::SingleQuotedString(s) => Ok(SqlLiteral::Str(s.into_boxed_str())),
        Value::HexStringLiteral(s) => Ok(SqlLiteral::Hex(s.into_boxed_str())),
        _ => Err(SqlUnsupported::Literal(value).into()),
    }
}

/// Parse an identifier
pub(crate) fn parse_ident(ObjectName(parts): ObjectName) -> SqlParseResult<SqlIdent> {
    parse_parts(parts)
}

/// Parse an identifier
pub(crate) fn parse_parts(mut parts: Vec<Ident>) -> SqlParseResult<SqlIdent> {
    if parts.len() == 1 {
        return Ok(parts.swap_remove(0).into());
    }
    Err(SqlUnsupported::MultiPartName(ObjectName(parts)).into())
}
