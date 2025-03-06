use std::sync::Arc;

use spacetimedb_lib::{query::Delta, AlgebraicType, AlgebraicValue};
use spacetimedb_primitives::TableId;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::{BinOp, LogOp};

/// A projection is the root of any relational expression.
/// This type represents a projection that returns relvars.
///
/// For example:
///
/// ```sql
/// select * from t
/// ```
///
/// and
///
/// ```sql
/// select t.* from t join s ...
/// ```
#[derive(Debug, PartialEq, Eq)]
pub enum ProjectName {
    None(RelExpr),
    Some(RelExpr, Box<str>),
}

impl ProjectName {
    /// Unwrap the outer projection, returning the inner expression
    pub fn unwrap(self) -> RelExpr {
        match self {
            Self::None(expr) | Self::Some(expr, _) => expr,
        }
    }

    /// What is the name of the return table?
    /// This is either the table name itself or its alias.
    pub fn return_name(&self) -> Option<&str> {
        match self {
            Self::None(input) => input.return_name(),
            Self::Some(_, name) => Some(name.as_ref()),
        }
    }

    /// The [TableSchema] of the returned rows.
    /// Note this expression returns rows from a relvar.
    /// Hence it this method should never return [None].
    pub fn return_table(&self) -> Option<&TableSchema> {
        match self {
            Self::None(input) => input.return_table(),
            Self::Some(input, alias) => input.find_table_schema(alias),
        }
    }

    /// The [TableId] of the returned rows.
    /// Note this expression returns rows from a relvar.
    /// Hence it this method should never return [None].
    pub fn return_table_id(&self) -> Option<TableId> {
        match self {
            Self::None(input) => input.return_table_id(),
            Self::Some(input, alias) => input.find_table_id(alias),
        }
    }

    /// Iterate over the returned column names and types
    pub fn for_each_return_field(&self, mut f: impl FnMut(&str, &AlgebraicType)) {
        if let Some(schema) = self.return_table() {
            for schema in schema.columns() {
                f(&schema.col_name, &schema.col_type);
            }
        }
    }
}

/// A projection is the root of any relational expression.
/// This type represents a projection that returns fields.
///
/// For example:
///
/// ```sql
/// select a, b from t
/// ```
///
/// and
///
/// ```sql
/// select t.a as x from t join s ...
/// ```
///
/// Note that RLS takes a single expression and produces a list of expressions.
/// Hence why these variants take lists rather than single expressions.
///
/// Why does RLS take an expression and produce a list?
///
/// There may be multiple RLS rules associated to a single table.
/// Semantically these rules represent a UNION over that table,
/// and this corresponds to a UNION in the original expression.
///
/// TODO: We should model the UNION explicitly in the physical plan.
///
/// Ex.
///
/// Let's say we have the following rules for the `users` table:
/// ```rust
/// use spacetimedb::client_visibility_filter;
/// use spacetimedb::Filter;
///
/// #[client_visibility_filter]
/// const USER_FILTER: Filter = Filter::Sql(
///     "SELECT users.* FROM users WHERE identity = :sender"
/// );
///
/// #[client_visibility_filter]
/// const ADMIN_FILTER: Filter = Filter::Sql(
///     "SELECT users.* FROM users JOIN admins"
/// );
/// ```
///
/// The user query
/// ```sql
/// SELECT * FROM users WHERE level > 5
/// ```
///
/// essentially resolves to
/// ```sql
/// SELECT users.*
/// FROM users
/// WHERE identity = :sender AND level > 5
///
/// UNION ALL
///
/// SELECT users.*
/// FROM users JOIN admins
/// WHERE users.level > 5
/// ```
#[derive(Debug)]
pub enum ProjectList {
    Name(Vec<ProjectName>),
    List(Vec<RelExpr>, Vec<(Box<str>, FieldProject)>),
    Limit(Box<ProjectList>, u64),
    Agg(Vec<RelExpr>, AggType, AlgebraicType),
}

#[derive(Debug)]
pub enum AggType {
    Count { alias: Box<str> },
}

impl ProjectList {
    /// Does this expression project a single relvar?
    /// If so, we return it's [TableSchema].
    /// If not, it projects a list of columns, so we return [None].
    pub fn return_table(&self) -> Option<&TableSchema> {
        match self {
            Self::Name(project) => project.first().and_then(|expr| expr.return_table()),
            Self::Limit(input, _) => input.return_table(),
            Self::List(..) | Self::Agg(..) => None,
        }
    }

    /// Does this expression project a single relvar?
    /// If so, we return it's [TableId].
    /// If not, it projects a list of columns, so we return [None].
    pub fn return_table_id(&self) -> Option<TableId> {
        match self {
            Self::Name(project) => project.first().and_then(|expr| expr.return_table_id()),
            Self::Limit(input, _) => input.return_table_id(),
            Self::List(..) | Self::Agg(..) => None,
        }
    }

    /// Iterate over the projected column names and types
    pub fn for_each_return_field(&self, mut f: impl FnMut(&str, &AlgebraicType)) {
        match self {
            Self::Name(input) => {
                input.first().inspect(|expr| expr.for_each_return_field(f));
            }
            Self::Limit(input, _) => {
                input.for_each_return_field(f);
            }
            Self::List(_, fields) => {
                for (name, FieldProject { ty, .. }) in fields {
                    f(name, ty);
                }
            }
            Self::Agg(_, AggType::Count { alias }, ty) => f(alias, ty),
        }
    }
}

/// A logical relational expression
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelExpr {
    /// A relvar or table reference
    RelVar(Relvar),
    /// A logical select for filter
    Select(Box<RelExpr>, Expr),
    /// A left deep binary cross product
    LeftDeepJoin(LeftDeepJoin),
    /// A left deep binary equi-join
    EqJoin(LeftDeepJoin, FieldProject, FieldProject),
}

/// A table reference
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relvar {
    /// The table schema of this relvar
    pub schema: Arc<TableSchema>,
    /// The name of this relvar
    pub alias: Box<str>,
    /// Does this relvar represent a delta table?
    pub delta: Option<Delta>,
}

impl RelExpr {
    /// Walk the expression tree and call `f` on each node
    pub fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        match self {
            Self::Select(lhs, _)
            | Self::LeftDeepJoin(LeftDeepJoin { lhs, .. })
            | Self::EqJoin(LeftDeepJoin { lhs, .. }, ..) => {
                lhs.visit(f);
            }
            Self::RelVar(..) => {}
        }
    }

    /// Walk the expression tree and call `f` on each node
    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        match self {
            Self::Select(lhs, _)
            | Self::LeftDeepJoin(LeftDeepJoin { lhs, .. })
            | Self::EqJoin(LeftDeepJoin { lhs, .. }, ..) => {
                lhs.visit_mut(f);
            }
            Self::RelVar(..) => {}
        }
    }

    /// The number of fields this expression returns
    pub fn nfields(&self) -> usize {
        match self {
            Self::RelVar(..) => 1,
            Self::LeftDeepJoin(join) | Self::EqJoin(join, ..) => join.lhs.nfields() + 1,
            Self::Select(input, _) => input.nfields(),
        }
    }

    /// Does this expression return this field?
    pub fn has_field(&self, field: &str) -> bool {
        match self {
            Self::RelVar(Relvar { alias, .. }) => alias.as_ref() == field,
            Self::LeftDeepJoin(join) | Self::EqJoin(join, ..) => {
                join.rhs.alias.as_ref() == field || join.lhs.has_field(field)
            }
            Self::Select(input, _) => input.has_field(field),
        }
    }

    /// Return the [TableSchema] for a relvar in the expression
    pub fn find_table_schema(&self, alias: &str) -> Option<&TableSchema> {
        match self {
            Self::RelVar(relvar) if relvar.alias.as_ref() == alias => Some(&relvar.schema),
            Self::Select(input, _) => input.find_table_schema(alias),
            Self::EqJoin(LeftDeepJoin { rhs, .. }, ..) if rhs.alias.as_ref() == alias => Some(&rhs.schema),
            Self::EqJoin(LeftDeepJoin { lhs, .. }, ..) => lhs.find_table_schema(alias),
            Self::LeftDeepJoin(LeftDeepJoin { rhs, .. }) if rhs.alias.as_ref() == alias => Some(&rhs.schema),
            Self::LeftDeepJoin(LeftDeepJoin { lhs, .. }) => lhs.find_table_schema(alias),
            _ => None,
        }
    }

    /// Return the [TableId] for a relvar in the expression
    pub fn find_table_id(&self, alias: &str) -> Option<TableId> {
        self.find_table_schema(alias).map(|schema| schema.table_id)
    }

    /// Does this expression return a single relvar?
    /// If so, return it's [TableSchema], otherwise return [None].
    pub fn return_table(&self) -> Option<&TableSchema> {
        match self {
            Self::RelVar(Relvar { schema, .. }) => Some(schema),
            Self::Select(input, _) => input.return_table(),
            _ => None,
        }
    }

    /// Does this expression return a single relvar?
    /// If so, return it's [TableId], otherwise return [None].
    pub fn return_table_id(&self) -> Option<TableId> {
        self.return_table().map(|schema| schema.table_id)
    }

    /// Does this expression return a single relvar?
    /// If so, return its name or equivalently its alias.
    pub fn return_name(&self) -> Option<&str> {
        match self {
            Self::RelVar(Relvar { alias, .. }) => Some(alias.as_ref()),
            Self::Select(input, _) => input.return_name(),
            _ => None,
        }
    }
}

/// A left deep binary cross product
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftDeepJoin {
    /// The lhs is recursive
    pub lhs: Box<RelExpr>,
    /// The rhs is a relvar
    pub rhs: Relvar,
}

/// A typed scalar expression
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A binary expression
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    /// A binary logic expression
    LogOp(LogOp, Box<Expr>, Box<Expr>),
    /// A typed literal expression
    Value(AlgebraicValue, AlgebraicType),
    /// A field projection
    Field(FieldProject),
}

impl Expr {
    /// Walk the expression tree and call `f` on each node
    pub fn visit(&self, f: &impl Fn(&Self)) {
        f(self);
        match self {
            Self::BinOp(_, a, b) | Self::LogOp(_, a, b) => {
                a.visit(f);
                b.visit(f);
            }
            Self::Value(..) | Self::Field(..) => {}
        }
    }

    /// Walk the expression tree and call `f` on each node
    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        match self {
            Self::BinOp(_, a, b) | Self::LogOp(_, a, b) => {
                a.visit_mut(f);
                b.visit_mut(f);
            }
            Self::Value(..) | Self::Field(..) => {}
        }
    }

    /// A literal boolean value
    pub const fn bool(v: bool) -> Self {
        Self::Value(AlgebraicValue::Bool(v), AlgebraicType::Bool)
    }

    /// A literal string value
    pub const fn str(v: Box<str>) -> Self {
        Self::Value(AlgebraicValue::String(v), AlgebraicType::String)
    }

    /// The [AlgebraicType] of this scalar expression
    pub fn ty(&self) -> &AlgebraicType {
        match self {
            Self::BinOp(..) | Self::LogOp(..) => &AlgebraicType::Bool,
            Self::Value(_, ty) | Self::Field(FieldProject { ty, .. }) => ty,
        }
    }
}

/// A typed qualified field projection
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProject {
    pub table: Box<str>,
    pub field: usize,
    pub ty: AlgebraicType,
}
