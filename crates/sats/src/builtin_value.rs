use crate::builtin_type::BuiltinType;
use crate::static_assert_size;
use crate::ArrayValue;
use enum_as_inner::EnumAsInner;
use itertools::Itertools;
use nonempty::NonEmpty;
use std::fmt;

/// Totally ordered [`f32`] allowing all IEEE-754 floating point values.
pub type F32 = decorum::Total<f32>;

/// Totally ordered [`f64`] allowing all IEEE-754 floating point values.
pub type F64 = decorum::Total<f64>;

/// A built-in value of a [`BuiltinType`].
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum BuiltinValue {
    /// A [`bool`] value of type [`BuiltinType::Bool`].
    Bool(bool),
    /// An [`i8`] value of type [`BuiltinType::I8`].
    I8(i8),
    /// A [`u8`] value of type [`BuiltinType::U8`].
    U8(u8),
    /// An [`i16`] value of type [`BuiltinType::I16`].
    I16(i16),
    /// A [`u16`] value of type [`BuiltinType::U16`].
    U16(u16),
    /// An [`i32`] value of type [`BuiltinType::I32`].
    I32(i32),
    /// A [`u32`] value of type [`BuiltinType::U32`].
    U32(u32),
    /// An [`i64`] value of type [`BuiltinType::I64`].
    I64(i64),
    /// A [`u64`] value of type [`BuiltinType::U64`].
    U64(u64),
    /// An [`i128`] value of type [`BuiltinType::I128`].
    I128(i128),
    /// A [`u128`] value of type [`BuiltinType::U128`].
    U128(u128),
    /// A totally ordered [`F32`] value of type [`BuiltinType::F32`].
    ///
    /// All floating point values defined in IEEE-754 are supported.
    /// However, unlike the primitive [`f32`], a [total order] is established.
    ///
    /// [total order]: https://docs.rs/decorum/0.3.1/decorum/#total-ordering
    F32(F32),
    /// A totally ordered [`F64`] value of type [`BuiltinType::F64`].
    ///
    /// All floating point values defined in IEEE-754 are supported.
    /// However, unlike the primitive [`f64`], a [total order] is established.
    ///
    /// [total order]: https://docs.rs/decorum/0.3.1/decorum/#total-ordering
    F64(F64),
    /// A UTF-8 string value of type [`BuiltinType::String`].
    ///
    /// Uses Rust's standard representation of strings.
    String(Box<str>),
    /// A homogeneous array of `AlgebraicValue`s.
    /// The array has the type [`BuiltinType::Array(elem_ty)`].
    ///
    /// The contained values are stored packed in a representation appropriate for their type.
    /// See [`ArrayValue`] for details on the representation.
    Array { val: ArrayValue },
}

static_assert_size!(BuiltinValue, 24);

impl BuiltinValue {
    /// Returns the byte string `v` as a [`BuiltinValue`].
    #[allow(non_snake_case)]
    pub const fn Bytes(v: Box<[u8]>) -> Self {
        Self::Array { val: ArrayValue::U8(v) }
    }

    /// Returns `self` as a borrowed byte string, if applicable.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BuiltinValue::Array { val: ArrayValue::U8(v) } => Some(v),
            _ => None,
        }
    }

    /// Converts `self` into a byte string, if applicable.
    pub fn into_bytes(self) -> Result<Box<[u8]>, Self> {
        match self {
            BuiltinValue::Array { val: ArrayValue::U8(v) } => Ok(v),
            _ => Err(self),
        }
    }
}

impl crate::Value for BuiltinValue {
    type Type = BuiltinType;
}
