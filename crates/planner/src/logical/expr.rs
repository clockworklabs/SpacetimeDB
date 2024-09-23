use std::fmt::{Display, Formatter};
use std::sync::Arc;

use spacetimedb_lib::AlgebraicValue;
use spacetimedb_sats::algebraic_type::fmt::{fmt_algebraic_type, fmt_product_type};
use spacetimedb_sats::AlgebraicType;
use spacetimedb_sats::ProductType;
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_sql_parser::ast::BinOp;

use crate::static_assert_size;

use super::bind::TypingResult;
use super::errors::{ConstraintViolation, ResolutionError, TypingError};

/// The type of a relation or scalar expression
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// A base relation
    Var(Arc<TableSchema>),
    /// A derived relation
    Row(ProductType),
    /// A join relation
    Tup(Box<[Type]>),
    /// A column type
    Alg(AlgebraicType),
}

static_assert_size!(Type, 24);

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alg(ty) => write!(f, "{}", fmt_algebraic_type(ty)),
            Self::Var(schema) => write!(f, "{}", fmt_product_type(schema.get_row_type())),
            Self::Row(ty) => write!(f, "{}", fmt_product_type(ty)),
            Self::Tup(types) => {
                write!(f, "(")?;
                write!(f, "{}", types[0])?;
                for t in &types[1..] {
                    write!(f, ", {}", t)?;
                }
                write!(f, ")")
            }
        }
    }
}

impl Type {
    /// A constant for the bool type
    pub const BOOL: Self = Self::Alg(AlgebraicType::Bool);

    /// A constant for the string type
    pub const STR: Self = Self::Alg(AlgebraicType::String);

    /// Is this a numeric type?
    pub fn is_num(&self) -> bool {
        match self {
            Self::Alg(t) => t.is_integer() || t.is_float(),
            _ => false,
        }
    }

    /// Is this a hex type?
    pub fn is_hex(&self) -> bool {
        match self {
            Self::Alg(t) => t.is_bytes() || t.is_identity() || t.is_address(),
            _ => false,
        }
    }

    /// Find a field and its position in a Row or Var type
    pub fn find(&self, field: &str) -> Option<(usize, &AlgebraicType)> {
        match self {
            Self::Var(schema) => schema
                .columns()
                .iter()
                .enumerate()
                .find(|(_, ColumnSchema { col_name, .. })| col_name.as_ref() == field)
                .map(|(i, ColumnSchema { col_type, .. })| (i, col_type)),
            Self::Row(row) => row
                .elements
                .iter()
                .enumerate()
                .find(|(_, elem)| elem.has_name(field))
                .map(|(i, elem)| (i, &elem.algebraic_type)),
            _ => None,
        }
    }
}

/// A logical relational expression
#[derive(Debug)]
pub enum RelExpr {
    /// A base table
    RelVar(Arc<TableSchema>, Type),
    /// A filter
    Select(Box<Select>),
    /// A projection
    Proj(Box<Project>),
    /// An n-ary join
    Join(Box<[RelExpr]>, Type),
    /// Bag union
    Union(Box<RelExpr>, Box<RelExpr>),
    /// Bag difference
    Minus(Box<RelExpr>, Box<RelExpr>),
    /// Bag -> set
    Dedup(Box<RelExpr>),
}

static_assert_size!(RelExpr, 40);

impl RelExpr {
    pub fn project(input: RelExpr, vars: Vars, refs: Vec<Ref>, ty: Type) -> Self {
        Self::Proj(Box::new(Project { input, vars, refs, ty }))
    }

    pub fn select(input: RelExpr, vars: Vars, exprs: Vec<Expr>) -> Self {
        Self::Select(Box::new(Select { input, vars, exprs }))
    }

    /// The type of a relation expression
    pub fn ty(&self) -> &Type {
        match self {
            Self::RelVar(_, ty) => ty,
            Self::Select(op) => op.input.ty(),
            Self::Proj(op) => &op.ty,
            Self::Join(_, ty) => ty,
            Self::Union(a, _) | Self::Minus(a, _) | Self::Dedup(a) => a.ty(),
        }
    }
}

/// A list of bound variables and their types.
/// Used as a typing context scoped to an expression.
#[derive(Debug, Clone, Default)]
pub struct Vars(Vec<(String, Type)>);

impl From<&TableSchema> for Vars {
    fn from(value: &TableSchema) -> Self {
        Self(
            value
                .columns()
                .iter()
                .map(|schema| (schema.col_name.to_string(), Type::Alg(schema.col_type.clone())))
                .collect(),
        )
    }
}

impl From<Vec<(String, Type)>> for Vars {
    fn from(vars: Vec<(String, Type)>) -> Self {
        Self(vars)
    }
}

impl<I: Into<(String, Type)>> FromIterator<I> for Vars {
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        Self(iter.into_iter().map(|item| item.into()).collect())
    }
}

impl Vars {
    /// Add a new variable binding to the list
    pub fn push(&mut self, var: (String, Type)) {
        self.0.push(var)
    }

    /// Returns an iterator over the variable names and types
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Type)> {
        self.0.iter().map(|(name, ty)| (name.as_str(), ty))
    }

    /// Find a variable by name in this list.
    /// Returns its type and position in the list if found.
    pub fn find(&self, param: &str) -> Option<(usize, &Type)> {
        self.0
            .iter()
            .enumerate()
            .find(|(_, (name, _))| name == param)
            .map(|(i, (_, ty))| (i, ty))
    }

    /// Find a variable from the list of in scope variables.
    /// Return its type and position in the list if found.
    /// Return an error otherwise.
    pub fn expect_var(&self, param: &str, expected: Option<&Type>) -> TypingResult<(usize, &Type)> {
        self.find(param)
            // Return resolution error if param not in scope
            .ok_or_else(|| ResolutionError::unresolved_var(param).into())
            // Return type error if param in scope but wrong type
            .and_then(|(i, ty)| match expected {
                Some(expected) if ty != expected => Err(ConstraintViolation::eq(expected, ty).into()),
                _ => Ok((i, ty)),
            })
    }

    /// Find an in scope table variable and field.
    /// Return its type, variable and field positions if found.
    /// Return an error otherwise.
    pub fn expect_field(
        &self,
        table: &str,
        field: &str,
        expected: Option<&Type>,
    ) -> TypingResult<(usize, usize, &AlgebraicType)> {
        self.find(table)
            // Return resolution error if table name not in scope
            .ok_or_else(|| TypingError::from(ResolutionError::unresolved_table(table)))
            .map(|(i, ty)| ty.find(field).map(|(j, ty)| (i, j, ty)))?
            // Return resolution error if field does not exist
            .ok_or_else(|| TypingError::from(ResolutionError::unresolved_field(table, field)))
            // Return type error if field exists but wrong type
            .and_then(|(i, j, ty)| match expected {
                Some(expected @ Type::Alg(want)) if ty != want => Err(TypingError::from(ConstraintViolation::eq(
                    expected,
                    &Type::Alg(ty.clone()),
                ))),
                _ => Ok((i, j, ty)),
            })
    }

    /// Find a param from the list of in scope variables.
    /// Return a variable reference expression if found.
    /// Return an error otherwise.
    pub fn expect_var_ref(&self, param: &str, expected: Option<&Type>) -> TypingResult<Expr> {
        self.expect_var(param, expected)
            .map(|(i, ty)| Expr::Ref(Ref::Var(i, ty.clone())))
    }

    /// Find an in scope table variable and field.
    /// Return a field reference expression if found.
    /// Return an error otherwise.
    pub fn expect_field_ref(&self, table: &str, field: &str, expected: Option<&Type>) -> TypingResult<Expr> {
        self.expect_field(table, field, expected)
            .map(|(i, j, ty)| Expr::Ref(Ref::Field(i, j, Type::Alg(ty.clone()))))
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
    pub ty: Type,
}

/// A typed scalar expression
#[derive(Debug)]
pub enum Expr {
    /// A binary expression
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// A typed literal expression
    Lit(AlgebraicValue, Type),
    /// A column or field reference
    Ref(Ref),
}

static_assert_size!(Expr, 48);

impl Expr {
    /// Returns a boolean literal
    pub const fn bool(v: bool) -> Self {
        Self::Lit(AlgebraicValue::Bool(v), Type::Alg(AlgebraicType::Bool))
    }

    /// Returns a string literal
    pub fn str(v: String) -> Self {
        let s = v.into_boxed_str();
        Self::Lit(AlgebraicValue::String(s), Type::Alg(AlgebraicType::String))
    }

    /// The type of a scalar expression
    pub fn ty(&self) -> &Type {
        match self {
            Self::Bin(..) => &Type::BOOL,
            Self::Lit(_, t) => t,
            Self::Ref(var) => var.ty(),
        }
    }
}

/// This type represents a column or field reference.
/// Note that variables are declared using an explicit parameter list.
/// Hence we store positions in that list rather than names.
#[derive(Debug)]
pub enum Ref {
    Var(usize, Type),
    Field(usize, usize, Type),
}

static_assert_size!(Ref, 40);

impl Ref {
    /// The type of this variable or field expression
    pub fn ty(&self) -> &Type {
        match self {
            Self::Var(_, t) | Self::Field(_, _, t) => t,
        }
    }
}
