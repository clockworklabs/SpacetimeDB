use std::fmt::{Display, Formatter};

use spacetimedb_lib::AlgebraicType;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sql_parser::ast::BinOp;
use thiserror::Error;

use crate::static_assert_size;

/// When type checking a [super::expr::RelExpr],
/// types are stored in a typing context [TyCtx].
/// It will then hold references, in the form of [TyId]s,
/// to the types defined in the [TyCtx].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TyId(u32);

impl TyId {
    /// The number of primitive types whose [TyId]s are statically defined.
    pub const N: usize = 16;

    /// The static type id for Bool
    pub const BOOL: Self = Self(0);

    /// The static type id for String
    pub const STR: Self = Self(15);

    /// Return the [Type] for this id with its typing context.
    /// Panics if the id is not valid for the context.
    pub fn with_ctx(self, ctx: &TyCtx) -> TypeWithCtx {
        TypeWithCtx(ctx.resolve(self), ctx)
    }

    /// Return the [Type] for this id with its typing context.
    /// Return an error if the id is not valid for the context.
    pub fn try_with_ctx(self, ctx: &TyCtx) -> Result<TypeWithCtx, InvalidTyId> {
        Ok(TypeWithCtx(ctx.try_resolve(self)?, ctx))
    }
}

impl Display for TyId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Error)]
#[error("Invalid type id {0}")]
pub struct InvalidTyId(TyId);

/// When type checking a [super::expr::RelExpr],
/// types are stored in a typing context [TyCtx].
/// It will then hold references, in the form of [TyId]s,
/// to the types defined in the [TyCtx].
pub struct TyCtx {
    types: Vec<Type>,
}

impl Default for TyCtx {
    fn default() -> Self {
        Self {
            types: vec![
                Type::Alg(AlgebraicType::Bool),
                Type::Alg(AlgebraicType::I8),
                Type::Alg(AlgebraicType::U8),
                Type::Alg(AlgebraicType::I16),
                Type::Alg(AlgebraicType::U16),
                Type::Alg(AlgebraicType::I32),
                Type::Alg(AlgebraicType::U32),
                Type::Alg(AlgebraicType::I64),
                Type::Alg(AlgebraicType::U64),
                Type::Alg(AlgebraicType::I128),
                Type::Alg(AlgebraicType::U128),
                Type::Alg(AlgebraicType::I256),
                Type::Alg(AlgebraicType::U256),
                Type::Alg(AlgebraicType::F32),
                Type::Alg(AlgebraicType::F64),
                Type::Alg(AlgebraicType::String),
            ],
        }
    }
}

impl TyCtx {
    /// Try to resolve an id to its [Type].
    /// Return a resolution error if not found.
    pub fn try_resolve(&self, id: TyId) -> Result<&Type, InvalidTyId> {
        self.types.get(id.0 as usize).ok_or(InvalidTyId(id))
    }

    /// Resolve an id to its [Type].
    /// Panics if id is out of bounds.
    pub fn resolve(&self, id: TyId) -> &Type {
        &self.types[id.0 as usize]
    }

    /// Add a type to the context and return a [TyId] for it.
    /// The [TyId] is not guaranteed to be unique to the type.
    /// However for primitive types it will be.
    pub fn add(&mut self, ty: Type) -> TyId {
        if let Type::Alg(t) = &ty {
            for i in 0..TyId::N {
                if let Type::Alg(s) = &self.types[i] {
                    if s == t {
                        return TyId(i as u32);
                    }
                }
            }
        }
        self.types.push(ty);
        let n = self.types.len() - 1;
        TyId(n as u32)
    }
}

/// A type wrapped with its typing context
pub struct TypeWithCtx<'a>(&'a Type, &'a TyCtx);

impl<'a> Eq for TypeWithCtx<'a> {}

/// A [TyId] is not guaranteed to be unique for a given [Type].
/// Hence we must fully resolve each [TyId] when testing for equality.
impl<'a> PartialEq for TypeWithCtx<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (Type::Var(a), Type::Var(b)) | (Type::Row(a), Type::Row(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .enumerate()
                        .all(|(i, (name, id))| name == &b[i].0 && id.with_ctx(self.1) == b[i].1.with_ctx(other.1))
            }
            (Type::Tup(a), Type::Tup(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .enumerate()
                        .all(|(i, id)| id.with_ctx(self.1) == b[i].with_ctx(other.1))
            }
            (Type::Alg(a), Type::Alg(b)) => a == b,
            _ => false,
        }
    }
}

impl<'a> Display for TypeWithCtx<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self(ty, ctx) = self;
        match ty {
            Type::Alg(ty) => write!(f, "{}", fmt_algebraic_type(ty)),
            Type::Var(fields) | Type::Row(fields) => {
                write!(f, "(")?;
                let (name, id) = &fields[0];
                write!(f, "{}: {}", name, id.with_ctx(ctx))?;
                for (name, id) in &fields[1..] {
                    write!(f, ", {}: {}", name, id.with_ctx(ctx))?;
                }
                write!(f, ")")
            }
            Type::Tup(types) => {
                write!(f, "(")?;
                write!(f, "{}", types[0].with_ctx(ctx))?;
                for id in &types[1..] {
                    write!(f, ", {}", id.with_ctx(ctx))?;
                }
                write!(f, ")")
            }
        }
    }
}

/// The type of a relation or scalar expression
pub enum Type {
    /// A base relation
    Var(Box<[(String, TyId)]>),
    /// A derived relation
    Row(Box<[(String, TyId)]>),
    /// A join relation
    Tup(Box<[TyId]>),
    /// A column type
    Alg(AlgebraicType),
}

static_assert_size!(Type, 24);

impl Type {
    /// A constant for the bool type
    pub const BOOL: Self = Self::Alg(AlgebraicType::Bool);

    /// A constant for the string type
    pub const STR: Self = Self::Alg(AlgebraicType::String);

    /// Wrap this type with its typing context
    pub fn with_ctx<'a>(&'a self, ctx: &'a TyCtx) -> TypeWithCtx {
        TypeWithCtx(self, ctx)
    }

    /// Is this type compatible with this binary operator?
    pub fn is_compatible_with(&self, op: BinOp) -> bool {
        match (op, self) {
            (BinOp::And | BinOp::Or, Type::Alg(AlgebraicType::Bool)) => true,
            (BinOp::And | BinOp::Or, _) => false,
            (BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte, Type::Alg(t)) => {
                t.is_bool()
                    || t.is_integer()
                    || t.is_float()
                    || t.is_string()
                    || t.is_bytes()
                    || t.is_identity()
                    || t.is_address()
            }
            (BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte, _) => false,
        }
    }

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
    pub fn find(&self, field: &str) -> Option<(usize, TyId)> {
        match self {
            Self::Var(schema) | Self::Row(schema) => schema
                .iter()
                .enumerate()
                .find(|(_, (name, _))| name == field)
                .map(|(i, (_, ty))| (i, *ty)),
            _ => None,
        }
    }
}
