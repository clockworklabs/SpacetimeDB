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
#[derive(Debug)]
pub enum ProjectName {
    None(RelExpr),
    Some(RelExpr, Box<str>),
}

impl ProjectName {
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
    pub fn iter_return_fields(&self, mut f: impl FnMut(&str, &AlgebraicType)) {
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
#[derive(Debug)]
pub enum ProjectList {
    Name(ProjectName),
    List(RelExpr, Vec<(Box<str>, FieldProject)>),
}

impl ProjectList {
    /// Does this expression project a single relvar?
    /// If so, we return it's [TableSchema].
    /// If not, it projects a list of columns, so we return [None].
    pub fn return_table(&self) -> Option<&TableSchema> {
        match self {
            Self::Name(project) => project.return_table(),
            Self::List(..) => None,
        }
    }

    /// Does this expression project a single relvar?
    /// If so, we return it's [TableId].
    /// If not, it projects a list of columns, so we return [None].
    pub fn return_table_id(&self) -> Option<TableId> {
        match self {
            Self::Name(project) => project.return_table_id(),
            Self::List(..) => None,
        }
    }

    /// Iterate over the projected column names and types
    pub fn iter_return_fields(&self, mut f: impl FnMut(&str, &AlgebraicType)) {
        match self {
            Self::Name(project) => {
                project.iter_return_fields(f);
            }
            Self::List(_, fields) => {
                for (name, FieldProject { ty, .. }) in fields {
                    f(name, ty);
                }
            }
        }
    }
}

/// A logical relational expression
#[derive(Debug)]
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
#[derive(Debug)]
pub struct Relvar {
    pub schema: Arc<TableSchema>,
    pub alias: Box<str>,
    /// Does this relvar represent a delta table?
    pub delta: Option<Delta>,
}

impl RelExpr {
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
}

/// A left deep binary cross product
#[derive(Debug)]
pub struct LeftDeepJoin {
    /// The lhs is recursive
    pub lhs: Box<RelExpr>,
    /// The rhs is a relvar
    pub rhs: Relvar,
}

/// A typed scalar expression
#[derive(Debug)]
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
#[derive(Debug)]
pub struct FieldProject {
    pub table: Box<str>,
    pub field: usize,
    pub ty: AlgebraicType,
}
