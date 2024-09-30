use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    ops::Deref,
};

use spacetimedb_lib::AlgebraicType;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sql_parser::ast::BinOp;
use string_interner::{backend::StringBackend, symbol::SymbolU32, StringInterner};
use thiserror::Error;

use crate::static_assert_size;

use super::errors::{ExpectedRelation, InvalidOp};

/// When type checking a [super::expr::RelExpr],
/// types are stored in a typing context [TyCtx].
/// It will then hold references, in the form of [TyId]s,
/// to the types defined in the [TyCtx].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TyId(u32);

impl TyId {
    /// The number of primitive types whose [TyId]s are statically defined.
    pub const N: usize = 18;

    /// The static type id for Bool
    /// The value is determined by [TyCtx::default()]
    pub const BOOL: Self = Self(0);

    /// The static type id for U64
    /// The value is determined by [TyCtx::default()]
    pub const U64: Self = Self(8);

    /// The static type id for String
    /// The value is determined by [TyCtx::default()]
    pub const STR: Self = Self(15);

    /// the static type id for a byte array
    /// The value is determined by [TyCtx::default()]
    pub const BYTES: Self = Self(16);

    /// Return the [Type] for this id with its typing context.
    /// Return an error if the id is not valid for the context.
    pub fn try_with_ctx(self, ctx: &TyCtx) -> Result<TypeWithCtx, InvalidTypeId> {
        ctx.try_resolve(self)
    }
}

impl Display for TyId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A symbol for names or identifiers in an expression tree
pub type Symbol = SymbolU32;

/// The type of a relation or scalar expression
#[derive(Debug)]
pub enum Type {
    /// A base relation
    Var(Box<[(Symbol, TyId)]>),
    /// A derived relation
    Row(Box<[(Symbol, TyId)]>),
    /// A column type
    Alg(AlgebraicType),
}

static_assert_size!(Type, 24);

impl Type {
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
}

/// When type checking a [super::expr::RelExpr],
/// types are stored in a typing context [TyCtx].
/// It will then hold references, in the form of [TyId]s,
/// to the types defined in the [TyCtx].
#[derive(Debug)]
pub struct TyCtx {
    types: Vec<Type>,
    names: StringInterner<StringBackend>,
}

impl Default for TyCtx {
    fn default() -> Self {
        Self {
            names: StringInterner::new(),
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
                Type::Alg(AlgebraicType::bytes()),
                Type::Alg(AlgebraicType::identity()),
            ],
        }
    }
}

#[derive(Debug, Error)]
#[error("Invalid type id {0}")]
pub struct InvalidTypeId(TyId);

impl TyCtx {
    /// Try to resolve an id to its [Type].
    /// Return a resolution error if not found.
    pub fn try_resolve(&self, id: TyId) -> Result<TypeWithCtx, InvalidTypeId> {
        self.types
            .get(id.0 as usize)
            .map(|ty| TypeWithCtx(ty, self))
            .ok_or(InvalidTypeId(id))
    }

    /// Resolve a [Symbol] to its name
    pub fn resolve_symbol(&self, id: Symbol) -> Option<&str> {
        self.names.resolve(id)
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

    /// Generate a [Symbol] from a string
    pub fn gen_symbol(&mut self, name: impl AsRef<str>) -> Symbol {
        self.names.get_or_intern(name)
    }

    /// Get an already generated [Symbol]
    pub fn get_symbol(&self, name: impl AsRef<str>) -> Option<Symbol> {
        self.names.get(name)
    }

    /// A wrapped [AlgebraicType::Bool] type
    pub fn bool(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::Alg(AlgebraicType::Bool), self)
    }

    /// A wrapped [AlgebraicType::String] type
    pub fn str(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::Alg(AlgebraicType::String), self)
    }

    /// A wrapped [AlgebraicType::U64] type
    pub fn u64(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::Alg(AlgebraicType::U64), self)
    }

    /// A wrapped [AlgebraicType::bytes()] type
    pub fn bytes(&self) -> TypeWithCtx {
        TypeWithCtx(&self.types[TyId::BYTES.0 as usize], self)
    }

    /// Are these types structurally equivalent?
    pub fn eq(&self, a: TyId, b: TyId) -> Result<bool, InvalidTypeId> {
        if a.0 < TyId::N as u32 || b.0 < TyId::N as u32 {
            return Ok(a == b);
        }
        match (&*self.try_resolve(a)?, &*self.try_resolve(b)?) {
            (Type::Alg(a), Type::Alg(b)) => Ok(a == b),
            (Type::Var(a), Type::Var(b)) | (Type::Row(a), Type::Row(b)) => Ok(a.len() == b.len() && {
                for (i, (name, id)) in a.iter().enumerate() {
                    if name != &b[i].0 || !self.eq(*id, b[i].1)? {
                        return Ok(false);
                    }
                }
                true
            }),
            _ => Ok(false),
        }
    }
}

/// A type wrapped with its typing context
#[derive(Debug)]
pub struct TypeWithCtx<'a>(&'a Type, &'a TyCtx);

impl Deref for TypeWithCtx<'_> {
    type Target = Type;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl TypeWithCtx<'_> {
    /// Expect a type compatible with this binary operator
    pub fn expect_op(&self, op: BinOp) -> Result<(), InvalidOp> {
        if self.0.is_compatible_with(op) {
            return Ok(());
        }
        Err(InvalidOp::new(op, self))
    }

    /// Expect a relvar or base relation type
    pub fn expect_relvar(&self) -> Result<RelType, ExpectedRelvar> {
        match self.0 {
            Type::Var(fields) => Ok(RelType { fields }),
            Type::Row(_) | Type::Alg(_) => Err(ExpectedRelvar),
        }
    }

    /// Expect a scalar or column type, not a relation type
    pub fn expect_scalar(&self) -> Result<&AlgebraicType, ExpectedScalar> {
        match self.0 {
            Type::Alg(t) => Ok(t),
            Type::Var(_) | Type::Row(_) => Err(ExpectedScalar),
        }
    }

    /// Expect a relation, not a scalar or column type
    pub fn expect_relation(&self) -> Result<RelType, ExpectedRelation> {
        match self.0 {
            Type::Var(fields) | Type::Row(fields) => Ok(RelType { fields }),
            Type::Alg(_) => Err(ExpectedRelation::new(self)),
        }
    }
}

/// The error type of [TypeWithCtx::expect_relvar()]
pub struct ExpectedRelvar;

/// The error type of [TypeWithCtx::expect_scalar()]
pub struct ExpectedScalar;

impl<'a> Display for TypeWithCtx<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Type::Alg(ty) => write!(f, "{}", fmt_algebraic_type(ty)),
            Type::Var(fields) | Type::Row(fields) => {
                const UNKNOWN: &str = "UNKNOWN";
                write!(f, "(")?;
                let (symbol, id) = &fields[0];
                let name = self.1.resolve_symbol(*symbol).unwrap_or(UNKNOWN);
                match self.1.try_resolve(*id) {
                    Ok(ty) => {
                        write!(f, "{}: {}", name, ty)?;
                    }
                    Err(_) => {
                        write!(f, "{}: {}", name, UNKNOWN)?;
                    }
                };
                for (symbol, id) in &fields[1..] {
                    let name = self.1.resolve_symbol(*symbol).unwrap_or(UNKNOWN);
                    match self.1.try_resolve(*id) {
                        Ok(ty) => {
                            write!(f, "{}: {}", name, ty)?;
                        }
                        Err(_) => {
                            write!(f, "{}: {}", name, UNKNOWN)?;
                        }
                    };
                }
                write!(f, ")")
            }
        }
    }
}

/// Represents a non-scalar or column type
#[derive(Debug)]
pub struct RelType<'a> {
    fields: &'a [(Symbol, TyId)],
}

impl<'a> RelType<'a> {
    /// Returns an iterator over the field names and types of this row type
    pub fn iter(&'a self) -> impl Iterator<Item = (usize, Symbol, TyId)> + '_ {
        self.fields.iter().enumerate().map(|(i, (name, ty))| (i, *name, *ty))
    }

    /// Find the position and type of a field in this row type if it exists
    pub fn find(&'a self, name: Symbol) -> Option<(usize, TyId)> {
        self.iter()
            .find(|(_, field, _)| *field == name)
            .map(|(i, _, ty)| (i, ty))
    }
}

/// A typing environment for an expression.
/// It binds in scope variables to their respective types.
#[derive(Debug, Clone, Default)]
pub struct TyEnv(HashMap<Symbol, TyId>);

impl TyEnv {
    /// Adds a new variable binding to the environment.
    /// Returns the old binding if the name was already in scope.
    pub fn add(&mut self, name: Symbol, ty: TyId) -> Option<TyId> {
        self.0.insert(name, ty)
    }

    /// Find a name in the environment
    pub fn find(&self, name: Symbol) -> Option<TyId> {
        self.0.get(&name).copied()
    }
}
