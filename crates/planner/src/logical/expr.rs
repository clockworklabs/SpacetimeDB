use std::sync::Arc;

use spacetimedb_lib::AlgebraicValue;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::BinOp;

use crate::static_assert_size;

use super::bind::TypingResult;
use super::errors::{ConstraintViolation, TypingError, Unresolved};
use super::ty::{InvalidTyId, TyCtx, TyId, Type};

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
    pub fn project(input: RelExpr, vars: Vars, refs: Vec<Ref>, ty: TyId) -> Self {
        Self::Proj(Box::new(Project { input, vars, refs, ty }))
    }

    /// Instantiate a selection [RelExpr::Select]
    pub fn select(input: RelExpr, vars: Vars, exprs: Vec<Expr>) -> Self {
        Self::Select(Box::new(Select { input, vars, exprs }))
    }

    /// The type id of this relation expression
    pub fn ty_id(&self) -> TyId {
        match self {
            Self::RelVar(_, id) | Self::Join(_, id) => *id,
            Self::Select(op) => op.input.ty_id(),
            Self::Proj(op) => op.ty,
            Self::Union(input, _) | Self::Minus(input, _) | Self::Dedup(input) => input.ty_id(),
        }
    }

    /// The [Type] of this relation expression
    pub fn ty<'a>(&self, ctx: &'a TyCtx) -> TypingResult<&'a Type> {
        ctx.try_resolve(self.ty_id()).map_err(TypingError::from)
    }
}

/// A list of bound variables and their types.
/// Used as a typing context scoped to an expression.
#[derive(Debug, Clone, Default)]
pub struct Vars(Vec<(String, TyId)>);

impl From<Vec<(String, TyId)>> for Vars {
    fn from(vars: Vec<(String, TyId)>) -> Self {
        Self(vars)
    }
}

impl Vars {
    /// Add a new variable binding to the list
    pub fn push(&mut self, var: (String, TyId)) {
        self.0.push(var)
    }

    /// Returns an iterator over the variable names and types
    pub fn iter(&self) -> impl Iterator<Item = (&str, &TyId)> {
        self.0.iter().map(|(name, ty)| (name.as_str(), ty))
    }

    /// Find a variable by name in this list.
    /// Returns its type and position in the list if found.
    pub fn find(&self, param: &str) -> Option<(usize, &TyId)> {
        self.0
            .iter()
            .enumerate()
            .find(|(_, (name, _))| name == param)
            .map(|(i, (_, ty))| (i, ty))
    }

    /// Find a variable from the list of in scope variables.
    /// Return its type and position in the list if found.
    /// Return an error otherwise.
    pub fn expect_var(&self, ctx: &TyCtx, param: &str, expected: Option<TyId>) -> TypingResult<(usize, TyId)> {
        self.find(param)
            // Return resolution error if param not in scope
            .ok_or_else(|| Unresolved::var(param).into())
            // Return type error if param in scope but wrong type
            .and_then(|(i, ty)| match expected {
                Some(expected) if *ty != expected => {
                    Err(ConstraintViolation::eq(expected.try_with_ctx(ctx)?, ty.try_with_ctx(ctx)?).into())
                }
                _ => Ok((i, *ty)),
            })
    }

    /// Find an in scope table variable and field.
    /// Return its type, variable and field positions if found.
    /// Return an error otherwise.
    pub fn expect_field(
        &self,
        ctx: &TyCtx,
        table: &str,
        field: &str,
        expected: Option<TyId>,
    ) -> TypingResult<(usize, usize, TyId)> {
        self.find(table)
            // Return resolution error if table name not in scope
            .ok_or_else(|| Unresolved::table(table))
            .map_err(TypingError::from)
            .and_then(|(i, id)| Ok((i, ctx.try_resolve(*id)?)))
            .map(|(i, ty)| ty.find(field).map(|(j, ty)| (i, j, ty)))?
            // Return resolution error if field does not exist
            .ok_or_else(|| Unresolved::field(table, field))
            .map_err(TypingError::from)
            // Return type error if field exists but wrong type
            .and_then(|(i, j, ty)| match expected {
                Some(id) if ty != id => Err(TypingError::from(ConstraintViolation::eq(
                    id.try_with_ctx(ctx)?,
                    ty.try_with_ctx(ctx)?,
                ))),
                _ => Ok((i, j, ty)),
            })
    }

    /// Find a param from the list of in scope variables.
    /// Return a variable reference expression if found.
    /// Return an error otherwise.
    pub fn expect_var_ref(&self, ctx: &TyCtx, param: &str, expected: Option<TyId>) -> TypingResult<Expr> {
        self.expect_var(ctx, param, expected)
            .map(|(i, ty)| Expr::Ref(Ref::Var(i, ty)))
    }

    /// Find an in scope table variable and field.
    /// Return a field reference expression if found.
    /// Return an error otherwise.
    pub fn expect_field_ref(
        &self,
        ctx: &TyCtx,
        table: &str,
        field: &str,
        expected: Option<TyId>,
    ) -> TypingResult<Expr> {
        self.expect_field(ctx, table, field, expected)
            .map(|(i, j, ty)| Expr::Ref(Ref::Field(i, j, ty)))
    }
}

/// A relational select operation or filter
#[derive(Debug)]
pub struct Select {
    /// The input relation
    pub input: RelExpr,
    /// The variables that are in scope
    pub vars: Vars,
    /// The predicate expression
    pub exprs: Vec<Expr>,
}

/// A relational project operation or map
#[derive(Debug)]
pub struct Project {
    /// The input relation
    pub input: RelExpr,
    /// The variables that are in scope
    pub vars: Vars,
    /// The projection expressions
    pub refs: Vec<Ref>,
    /// The type of the output relation
    pub ty: TyId,
}

/// A typed scalar expression
#[derive(Debug)]
pub enum Expr {
    /// A binary expression
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// A typed literal expression
    Lit(AlgebraicValue, TyId),
    /// A column or field reference
    Ref(Ref),
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
            Self::Lit(_, id) => *id,
            Self::Ref(var) => var.ty_id(),
        }
    }

    /// The [Type] of this expression
    pub fn ty<'a>(&self, ctx: &'a TyCtx) -> Result<&'a Type, InvalidTyId> {
        ctx.try_resolve(self.ty_id())
    }
}

/// This type represents a column or field reference.
/// Note that variables are declared using an explicit parameter list.
/// Hence we store positions in that list rather than names.
#[derive(Debug)]
pub enum Ref {
    Var(usize, TyId),
    Field(usize, usize, TyId),
}

impl Ref {
    /// The type id of this variable or field expression
    pub fn ty_id(&self) -> TyId {
        match self {
            Self::Var(_, id) | Self::Field(_, _, id) => *id,
        }
    }

    /// The [Type] of this variable or field expression
    pub fn ty<'a>(&self, ctx: &'a TyCtx) -> Result<&'a Type, InvalidTyId> {
        ctx.try_resolve(self.ty_id())
    }
}
