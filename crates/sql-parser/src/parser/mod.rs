use errors::{SqlParseError, SqlRequired, SqlUnsupported};
use sqlparser::ast::{
    BinaryOperator, Expr, Ident, Join, JoinConstraint, JoinOperator, ObjectName, Query, SelectItem, TableAlias,
    TableFactor, TableWithJoins, Value, WildcardAdditionalOptions,
};

use crate::ast::{BinOp, Project, ProjectElem, RelExpr, SqlExpr, SqlFrom, SqlIdent, SqlJoin, SqlLiteral};

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
    fn parse_from(mut tables: Vec<TableWithJoins>) -> SqlParseResult<SqlFrom<Self::Ast>> {
        if tables.is_empty() {
            return Err(SqlRequired::From.into());
        }
        if tables.len() > 1 {
            return Err(SqlUnsupported::ImplicitJoins.into());
        }
        let TableWithJoins { relation, joins } = tables.swap_remove(0);
        let (expr, alias) = Self::parse_rel(relation)?;
        if joins.is_empty() {
            return Ok(SqlFrom::Expr(expr, alias));
        }
        let (expr, alias) = Self::parse_alias((expr, alias))?;
        Ok(SqlFrom::Join(expr, alias, Self::parse_joins(joins)?))
    }

    /// Parse a sequence of JOIN clauses
    fn parse_joins(joins: Vec<Join>) -> SqlParseResult<Vec<SqlJoin<Self::Ast>>> {
        joins.into_iter().map(Self::parse_join).collect()
    }

    /// Parse a single JOIN clause
    fn parse_join(join: Join) -> SqlParseResult<SqlJoin<Self::Ast>> {
        let (expr, alias) = Self::parse_alias(Self::parse_rel(join.relation)?)?;
        match join.join_operator {
            JoinOperator::CrossJoin => Ok(SqlJoin { expr, alias, on: None }),
            JoinOperator::Inner(JoinConstraint::None) => Ok(SqlJoin { expr, alias, on: None }),
            JoinOperator::Inner(JoinConstraint::On(on)) => Ok(SqlJoin {
                expr,
                alias,
                on: Some(parse_expr(on)?),
            }),
            _ => Err(SqlUnsupported::JoinType.into()),
        }
    }

    /// Check optional and required table aliases in a JOIN clause
    fn parse_alias(item: (RelExpr<Self::Ast>, Option<SqlIdent>)) -> SqlParseResult<(RelExpr<Self::Ast>, SqlIdent)> {
        match item {
            (RelExpr::Var(alias), None) => Ok((RelExpr::Var(alias.clone()), alias)),
            (expr, Some(alias)) => Ok((expr, alias)),
            _ => Err(SqlRequired::JoinAlias.into()),
        }
    }

    /// Parse a relation expression in a FROM clause
    fn parse_rel(expr: TableFactor) -> SqlParseResult<(RelExpr<Self::Ast>, Option<SqlIdent>)> {
        match expr {
            // Relvar no alias
            TableFactor::Table {
                name,
                alias: None,
                args: None,
                with_hints,
                version: None,
                partitions,
            } if with_hints.is_empty() && partitions.is_empty() => Ok((RelExpr::Var(parse_ident(name)?), None)),
            // Relvar with alias
            TableFactor::Table {
                name,
                alias: Some(TableAlias { name: alias, columns }),
                args: None,
                with_hints,
                version: None,
                partitions,
            } if with_hints.is_empty() && partitions.is_empty() && columns.is_empty() => {
                Ok((RelExpr::Var(parse_ident(name)?), Some(alias.into())))
            }
            // RelExpr no alias
            TableFactor::Derived {
                lateral: false,
                subquery,
                alias: None,
            } => Ok((RelExpr::Ast(Box::new(Self::parse_query(*subquery)?)), None)),
            // RelExpr with alias
            TableFactor::Derived {
                lateral: false,
                subquery,
                alias: Some(TableAlias { name, columns }),
            } if columns.is_empty() => Ok((RelExpr::Ast(Box::new(Self::parse_query(*subquery)?)), Some(name.into()))),
            _ => Err(SqlUnsupported::From(expr).into()),
        }
    }
}

/// Parse the items of a SELECT clause
pub(crate) fn parse_projection(mut items: Vec<SelectItem>) -> SqlParseResult<Project> {
    if items.len() == 1 {
        return parse_project(items.swap_remove(0));
    }
    Ok(Project::Exprs(
        items
            .into_iter()
            .map(parse_project_elem)
            .collect::<SqlParseResult<_>>()?,
    ))
}

/// Parse a SELECT clause with only a single item
pub(crate) fn parse_project(item: SelectItem) -> SqlParseResult<Project> {
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
        SelectItem::UnnamedExpr(_) | SelectItem::ExprWithAlias { .. } => {
            Ok(Project::Exprs(vec![parse_project_elem(item)?]))
        }
        item => Err(SqlUnsupported::Projection(item).into()),
    }
}

/// Parse an item in a SELECT clause
pub(crate) fn parse_project_elem(item: SelectItem) -> SqlParseResult<ProjectElem> {
    match item {
        SelectItem::Wildcard(_) => Err(SqlUnsupported::MixedWildcardProject.into()),
        SelectItem::QualifiedWildcard(..) => Err(SqlUnsupported::MixedWildcardProject.into()),
        SelectItem::UnnamedExpr(expr) => Ok(ProjectElem(parse_expr(expr)?, None)),
        SelectItem::ExprWithAlias { expr, alias } => Ok(ProjectElem(parse_expr(expr)?, Some(alias.into()))),
    }
}

/// Parse a scalar expression
pub(crate) fn parse_expr(expr: Expr) -> SqlParseResult<SqlExpr> {
    match expr {
        Expr::Value(v) => Ok(SqlExpr::Lit(parse_literal(v)?)),
        Expr::Identifier(ident) => Ok(SqlExpr::Var(ident.into())),
        Expr::CompoundIdentifier(mut idents) if idents.len() == 2 => {
            let table = idents.swap_remove(0).into();
            let field = idents.swap_remove(0).into();
            Ok(SqlExpr::Field(table, field))
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
        BinaryOperator::And => Ok(BinOp::And),
        BinaryOperator::Or => Ok(BinOp::Or),
        _ => Err(SqlUnsupported::BinOp(op).into()),
    }
}

/// Parse a literal expression
pub(crate) fn parse_literal(value: Value) -> SqlParseResult<SqlLiteral> {
    match value {
        Value::Boolean(v) => Ok(SqlLiteral::Bool(v)),
        Value::Number(v, _) => Ok(SqlLiteral::Num(v)),
        Value::SingleQuotedString(s) => Ok(SqlLiteral::Str(s)),
        Value::HexStringLiteral(s) => Ok(SqlLiteral::Hex(s)),
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
