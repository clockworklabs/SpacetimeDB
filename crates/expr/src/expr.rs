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
    /// What is the [TableId] for this projection?
    pub fn table_id(&self) -> Option<TableId> {
        match self {
            Self::None(input) => input.table_id(None),
            Self::Some(input, var) => input.table_id(Some(var.as_ref())),
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
    /// What is the [TableId] for this projection?
    pub fn table_id(&self) -> Option<TableId> {
        match self {
            Self::List(..) => None,
            Self::Name(proj) => proj.table_id(),
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

    /// What is the [TableId] for this expression or relvar?
    pub fn table_id(&self, var: Option<&str>) -> Option<TableId> {
        match (self, var) {
            (Self::RelVar(Relvar { schema, .. }), None) => Some(schema.table_id),
            (Self::RelVar(Relvar { schema, alias, .. }), Some(var)) if alias.as_ref() == var => Some(schema.table_id),
            (Self::RelVar(Relvar { schema, .. }), Some(_)) => Some(schema.table_id),
            (Self::Select(input, _), _) => input.table_id(var),
            (Self::LeftDeepJoin(..) | Self::EqJoin(..), None) => None,
            (Self::LeftDeepJoin(join) | Self::EqJoin(join, ..), Some(name)) => {
                if join.rhs.alias.as_ref() == name {
                    Some(join.rhs.schema.table_id)
                } else {
                    join.lhs.table_id(var)
                }
            }
        }
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
