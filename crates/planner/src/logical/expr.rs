use std::sync::Arc;

use spacetimedb_lib::AlgebraicValue;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::BinOp;

use crate::static_assert_size;

use super::ty::{InvalidTypeId, Symbol, TyCtx, TyId, TypeWithCtx};

/// A logical relational expression
#[derive(Debug)]
pub enum RelExpr {
    /// A base table
    RelVar(Arc<TableSchema>, TyId),
    /// A filter
    Select(Box<Select>),
    /// A projection
    Proj(Box<Project>),
    /// An n-ary join
    Join(Box<[RelExpr]>, TyId),
    /// Bag union
    Union(Box<RelExpr>, Box<RelExpr>),
    /// Bag difference
    Minus(Box<RelExpr>, Box<RelExpr>),
    /// Bag -> set
    Dedup(Box<RelExpr>),
}

static_assert_size!(RelExpr, 24);

impl RelExpr {
    /// Instantiate a projection [RelExpr::Proj]
    pub fn project(input: RelExpr, expr: Let) -> Self {
        Self::Proj(Box::new(Project { input, expr }))
    }

    /// Instantiate a selection [RelExpr::Select]
    pub fn select(input: RelExpr, expr: Let) -> Self {
        Self::Select(Box::new(Select { input, expr }))
    }

    /// The type id of this relation expression
    pub fn ty_id(&self) -> TyId {
        match self {
            Self::RelVar(_, id) | Self::Join(_, id) => *id,
            Self::Select(op) => op.input.ty_id(),
            Self::Proj(op) => op.expr.exprs[0].ty_id(),
            Self::Union(input, _) | Self::Minus(input, _) | Self::Dedup(input) => input.ty_id(),
        }
    }

    /// The type of this relation expression
    pub fn ty<'a>(&self, ctx: &'a TyCtx) -> Result<TypeWithCtx<'a>, InvalidTypeId> {
        ctx.try_resolve(self.ty_id())
    }
}

/// A relational select operation or filter
#[derive(Debug)]
pub struct Select {
    /// The input relation
    pub input: RelExpr,
    /// The predicate expression
    pub expr: Let,
}

/// A relational project operation or map
#[derive(Debug)]
pub struct Project {
    /// The input relation
    pub input: RelExpr,
    /// The projection expression
    pub expr: Let,
}

/// Let variables for selections and projections.
///
/// Relational operators take a single input paramter.
/// Let variables explicitly destructure the input row.
#[derive(Debug)]
pub struct Let {
    /// The variable definitions for this let expression
    pub vars: Vec<(Symbol, Expr)>,
    /// The expressions for which the above variables are in scope
    pub exprs: Vec<Expr>,
}

/// A typed scalar expression
#[derive(Debug)]
pub enum Expr {
    /// A binary expression
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// A variable reference
    Var(Symbol, TyId),
    /// A row or projection expression
    Row(Box<[(Symbol, Expr)]>, TyId),
    /// A typed literal expression
    Lit(AlgebraicValue, TyId),
    /// A field expression
    Field(Box<Expr>, usize, TyId),
    /// The input parameter to a relop
    Input(TyId),
}

static_assert_size!(Expr, 32);

impl Expr {
    /// Returns a boolean literal
    pub const fn bool(v: bool) -> Self {
        Self::Lit(AlgebraicValue::Bool(v), TyId::BOOL)
    }

    /// Returns a string literal
    pub fn str(v: String) -> Self {
        let s = v.into_boxed_str();
        Self::Lit(AlgebraicValue::String(s), TyId::STR)
    }

    /// The type id of this expression
    pub fn ty_id(&self) -> TyId {
        match self {
            Self::Bin(..) => TyId::BOOL,
            Self::Lit(_, id) | Self::Var(_, id) | Self::Input(id) | Self::Field(_, _, id) | Self::Row(_, id) => *id,
        }
    }

    /// The type of this expression
    pub fn ty<'a>(&self, ctx: &'a TyCtx) -> Result<TypeWithCtx<'a>, InvalidTypeId> {
        ctx.try_resolve(self.ty_id())
    }
}
