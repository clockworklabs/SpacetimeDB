use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    ops::Deref,
};

use super::errors::{ExpectedRelation, InvalidOp};
use crate::StatementSource;
use spacetimedb_lib::AlgebraicType;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sql_parser::ast::BinOp;
use string_interner::{backend::StringBackend, symbol::SymbolU32, StringInterner};
use thiserror::Error;

/// When type checking a [super::expr::RelExpr],
/// types are stored in a typing context [TyCtx].
/// It will then hold references, in the form of [TyId]s,
/// to the types defined in the [TyCtx].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TyId(u32);

impl TyId {
    /// A static type id for Bool
    pub const BOOL: Self = Self(0);

    /// A static type id for I8
    pub const I8: Self = Self(1);

    /// A static type id for U8
    pub const U8: Self = Self(2);

    /// A static type id for I16
    pub const I16: Self = Self(3);

    /// A static type id for U16
    pub const U16: Self = Self(4);

    /// A static type id for I32
    pub const I32: Self = Self(5);

    /// A static type id for U32
    pub const U32: Self = Self(6);

    /// A static type id for I64
    pub const I64: Self = Self(7);

    /// A static type id for U64
    pub const U64: Self = Self(8);

    /// A static type id for I128
    pub const I128: Self = Self(9);

    /// A static type id for U128
    pub const U128: Self = Self(10);

    /// A static type id for I256
    pub const I256: Self = Self(11);

    /// A static type id for U256
    pub const U256: Self = Self(12);

    /// A static type id for F32
    pub const F32: Self = Self(13);

    /// A static type id for F64
    pub const F64: Self = Self(14);

    /// A static type id for String
    pub const STR: Self = Self(15);

    /// A static type id for a byte array
    pub const BYTES: Self = Self(16);

    /// A static type id for [AlgebraicType::identity()]
    pub const IDENT: Self = Self(17);

    /// The number of statically defined [TyId]s
    const N: usize = 18;
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
    Var(TableId, Box<[(Symbol, TyId)]>),
    /// A derived relation
    Row(Box<[(Symbol, TyId)]>),
    /// A column type
    Alg(AlgebraicType),
}

impl Type {
    /// A constant for the primitive type Bool
    pub const BOOL: Self = Self::Alg(AlgebraicType::Bool);

    /// A constant for the primitive type I8
    pub const I8: Self = Self::Alg(AlgebraicType::I8);

    /// A constant for the primitive type U8
    pub const U8: Self = Self::Alg(AlgebraicType::U8);

    /// A constant for the primitive type I16
    pub const I16: Self = Self::Alg(AlgebraicType::I16);

    /// A constant for the primitive type U16
    pub const U16: Self = Self::Alg(AlgebraicType::U16);

    /// A constant for the primitive type I32
    pub const I32: Self = Self::Alg(AlgebraicType::I32);

    /// A constant for the primitive type U32
    pub const U32: Self = Self::Alg(AlgebraicType::U32);

    /// A constant for the primitive type I64
    pub const I64: Self = Self::Alg(AlgebraicType::I64);

    /// A constant for the primitive type U64
    pub const U64: Self = Self::Alg(AlgebraicType::U64);

    /// A constant for the primitive type I128
    pub const I128: Self = Self::Alg(AlgebraicType::I128);

    /// A constant for the primitive type U128
    pub const U128: Self = Self::Alg(AlgebraicType::U128);

    /// A constant for the primitive type I256
    pub const I256: Self = Self::Alg(AlgebraicType::I256);

    /// A constant for the primitive type U256
    pub const U256: Self = Self::Alg(AlgebraicType::U256);

    /// A constant for the primitive type F32
    pub const F32: Self = Self::Alg(AlgebraicType::F32);

    /// A constant for the primitive type F64
    pub const F64: Self = Self::Alg(AlgebraicType::F64);

    /// A constant for the primitive type String
    pub const STR: Self = Self::Alg(AlgebraicType::String);

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
    /// A statically interned byte array type
    bytes: Type,
    /// A statically interned identity type
    ident: Type,
    /// Types that are interned dynamically during type checking
    types: Vec<Type>,
    /// Interned identifiers
    names: StringInterner<StringBackend>,
    /// The source of the statement being type checked
    pub source: StatementSource,
}

impl Default for TyCtx {
    fn default() -> Self {
        Self {
            // Pre-intern the byte array type
            bytes: Type::Alg(AlgebraicType::bytes()),
            // Pre-intern the identity type
            ident: Type::Alg(AlgebraicType::identity()),
            // All other composite types are interned on the fly
            types: vec![],
            // Intern identifiers on the fly
            names: StringInterner::new(),
            // Default to a subscription source, because is more restrictive
            source: StatementSource::Subscription,
        }
    }
}

#[derive(Debug, Error)]
#[error("Invalid type id {0}")]
pub struct InvalidTypeId(TyId);

impl TyCtx {
    /// Return a wrapped [Type::BOOL]
    pub fn bool(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::BOOL, self)
    }

    /// Return a wrapped [Type::I8]
    pub fn i8(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::I8, self)
    }

    /// Return a wrapped [Type::U8]
    pub fn u8(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::U8, self)
    }

    /// Return a wrapped [Type::I16]
    pub fn i16(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::I16, self)
    }

    /// Return a wrapped [Type::U16]
    pub fn u16(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::U16, self)
    }

    /// Return a wrapped [Type::I32]
    pub fn i32(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::I32, self)
    }

    /// Return a wrapped [Type::U32]
    pub fn u32(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::U32, self)
    }

    /// Return a wrapped [Type::I64]
    pub fn i64(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::I64, self)
    }

    /// Return a wrapped [Type::U64]
    pub fn u64(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::U64, self)
    }

    /// Return a wrapped [Type::I128]
    pub fn i128(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::I128, self)
    }

    /// Return a wrapped [Type::U128]
    pub fn u128(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::U128, self)
    }

    /// Return a wrapped [Type::I256]
    pub fn i256(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::I256, self)
    }

    /// Return a wrapped [Type::U256]
    pub fn u256(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::U256, self)
    }

    /// Return a wrapped [Type::F32]
    pub fn f32(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::F32, self)
    }

    /// Return a wrapped [Type::F64]
    pub fn f64(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::F64, self)
    }

    /// Return a wrapped [Type::STR]
    pub fn str(&self) -> TypeWithCtx {
        TypeWithCtx(&Type::STR, self)
    }

    /// Return a wrapped [AlgebraicType::bytes()]
    pub fn bytes(&self) -> TypeWithCtx {
        TypeWithCtx(&self.bytes, self)
    }

    /// Return a wrapped [AlgebraicType::identity()]
    pub fn ident(&self) -> TypeWithCtx {
        TypeWithCtx(&self.ident, self)
    }

    /// Try to resolve an id to its [Type].
    /// Return a resolution error if not found.
    pub fn try_resolve(&self, id: TyId) -> Result<TypeWithCtx, InvalidTypeId> {
        match id {
            TyId::BOOL => {
                // Resolve the primitive type Bool
                Ok(self.bool())
            }
            TyId::I8 => {
                // Resolve the primitive type I8
                Ok(self.i8())
            }
            TyId::U8 => {
                // Resolve the primitive type U8
                Ok(self.u8())
            }
            TyId::I16 => {
                // Resolve the primitive type I16
                Ok(self.i16())
            }
            TyId::U16 => {
                // Resolve the primitive type U16
                Ok(self.u16())
            }
            TyId::I32 => {
                // Resolve the primitive type I32
                Ok(self.i32())
            }
            TyId::U32 => {
                // Resolve the primitive type U32
                Ok(self.u32())
            }
            TyId::I64 => {
                // Resolve the primitive type I64
                Ok(self.i64())
            }
            TyId::U64 => {
                // Resolve the primitive type U64
                Ok(self.u64())
            }
            TyId::I128 => {
                // Resolve the primitive type I128
                Ok(self.i128())
            }
            TyId::U128 => {
                // Resolve the primitive type U128
                Ok(self.u128())
            }
            TyId::I256 => {
                // Resolve the primitive type I256
                Ok(self.i256())
            }
            TyId::U256 => {
                // Resolve the primitive type U256
                Ok(self.u256())
            }
            TyId::F32 => {
                // Resolve the primitive type F32
                Ok(self.f32())
            }
            TyId::F64 => {
                // Resolve the primitive type F64
                Ok(self.f64())
            }
            TyId::STR => {
                // Resolve the primitive type String
                Ok(self.str())
            }
            TyId::BYTES => {
                // Resolve the byte array type
                Ok(self.bytes())
            }
            TyId::IDENT => {
                // Resolve the special identity type
                Ok(self.ident())
            }
            _ => self
                .types
                .get(id.0 as usize - TyId::N)
                .map(|ty| TypeWithCtx(ty, self))
                .ok_or(InvalidTypeId(id)),
        }
    }

    /// Resolve a [Symbol] to its name
    pub fn resolve_symbol(&self, id: Symbol) -> Option<&str> {
        self.names.resolve(id)
    }

    /// Add an [AlgebraicType] to the context and return a [TyId] for it.
    /// The [TyId] is not guaranteed to be unique to the type.
    /// However for primitive types it will be.
    pub fn add_algebraic_type(&mut self, ty: &AlgebraicType) -> TyId {
        match ty {
            AlgebraicType::Bool => {
                // Bool -> BOOL
                TyId::BOOL
            }
            AlgebraicType::I8 => {
                // I8 -> I8
                TyId::I8
            }
            AlgebraicType::U8 => {
                // U8 -> U8
                TyId::U8
            }
            AlgebraicType::I16 => {
                // I16 -> I16
                TyId::I16
            }
            AlgebraicType::U16 => {
                // U16 -> U16
                TyId::U16
            }
            AlgebraicType::I32 => {
                // I32 -> I32
                TyId::I32
            }
            AlgebraicType::U32 => {
                // U32 -> U32
                TyId::U32
            }
            AlgebraicType::I64 => {
                // I64 -> I64
                TyId::I64
            }
            AlgebraicType::U64 => {
                // U64 -> U64
                TyId::U64
            }
            AlgebraicType::I128 => {
                // I128 -> I128
                TyId::I128
            }
            AlgebraicType::U128 => {
                // U128 -> U128
                TyId::U128
            }
            AlgebraicType::I256 => {
                // I256 -> I256
                TyId::I256
            }
            AlgebraicType::U256 => {
                // U256 -> U256
                TyId::U256
            }
            AlgebraicType::F32 => {
                // F32 -> F32
                TyId::F32
            }
            AlgebraicType::F64 => {
                // F64 -> F64
                TyId::F64
            }
            AlgebraicType::String => {
                // String -> STR
                TyId::STR
            }
            AlgebraicType::Array(ty) if ty.elem_ty.is_u8() => {
                // [u8] -> BYTES
                TyId::BYTES
            }
            AlgebraicType::Product(ty) if ty.is_identity() => {
                // { __identity_bytes: [u8] } -> IDENT
                TyId::IDENT
            }
            _ => {
                let n = self.types.len() + TyId::N;
                self.types.push(Type::Alg(ty.clone()));
                TyId(n as u32)
            }
        }
    }

    /// Add a relvar or table type to the context and return a [TyId] for it.
    /// The [TyId] is not guaranteed to be unique to the type.
    pub fn add_var_type(&mut self, table_id: TableId, fields: Vec<(Symbol, TyId)>) -> TyId {
        let n = self.types.len() + TyId::N;
        self.types.push(Type::Var(table_id, fields.into_boxed_slice()));
        TyId(n as u32)
    }

    /// Add a derived row type to the context and return a [TyId] for it.
    /// The [TyId] is not guaranteed to be unique to the type.
    pub fn add_row_type(&mut self, fields: Vec<(Symbol, TyId)>) -> TyId {
        let n = self.types.len() + TyId::N;
        self.types.push(Type::Row(fields.into_boxed_slice()));
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

    /// Are these rows structurally equivalent?
    fn eq_row(&self, a: &[(Symbol, TyId)], b: &[(Symbol, TyId)]) -> bool {
        a.len() == b.len() && {
            for (i, (name, id)) in a.iter().enumerate() {
                if name != &b[i].0 || !self.eq(*id, b[i].1).unwrap() {
                    return false;
                }
            }
            true
        }
    }

    /// Are these types structurally equivalent?
    pub fn eq(&self, a: TyId, b: TyId) -> Result<bool, InvalidTypeId> {
        if a.0 < TyId::N as u32 || b.0 < TyId::N as u32 {
            return Ok(a == b);
        }
        match (&*self.try_resolve(a)?, &*self.try_resolve(b)?) {
            (Type::Alg(a), Type::Alg(b)) => Ok(a == b),
            // UNION is not valid for subscriptions
            (Type::Var(a, row_a), Type::Var(b, row_b)) => match self.source {
                StatementSource::Subscription => Ok(a == b),
                StatementSource::Query => Ok(a == b || self.eq_row(row_a, row_b)),
            },
            (Type::Row(a), Type::Row(b)) => Ok(self.eq_row(a, b)),
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
            Type::Var(_, fields) => Ok(RelType { fields }),
            Type::Row(_) | Type::Alg(_) => Err(ExpectedRelvar),
        }
    }

    /// Expect a scalar or column type, not a relation type
    pub fn expect_scalar(&self) -> Result<&AlgebraicType, ExpectedScalar> {
        match self.0 {
            Type::Alg(t) => Ok(t),
            Type::Var(..) | Type::Row(..) => Err(ExpectedScalar),
        }
    }

    /// Expect a relation, not a scalar or column type
    pub fn expect_relation(&self) -> Result<RelType, ExpectedRelation> {
        match self.0 {
            Type::Var(_, fields) | Type::Row(fields) => Ok(RelType { fields }),
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
            Type::Var(_, fields) | Type::Row(fields) => {
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
