use crate::algebraic_value::{F32, F64};
use crate::builtin_type::BuiltinType;
use crate::{ArrayValue, MapValue};
use enum_as_inner::EnumAsInner;

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
    String(String),
    /// A homogeneous array of `AlgebraicValue`s.
    /// The array has the type [`BuiltinType::Array(elem_ty)`].
    ///
    /// The contained values are stored packed in a representation appropriate for their type.
    /// See [`ArrayValue`] for details on the representation.
    Array { val: ArrayValue },
    /// An ordered map value of `key: AlgebraicValue`s mapped to `value: AlgebraicValue`s.
    /// Each `key` must be of the same [`AlgebraicType`] as all the others
    /// and the same applies to each `value`.
    /// A map as a whole has the type [`BuiltinType::Map(key_ty, val_ty)`].
    ///
    /// Maps are implemented internally as [`BTreeMap<AlgebraicValue, AlgebraicValue>`].
    /// This implies that key/values are ordered first by key and then value
    /// as if they were a sorted slice `[(key, value)]`.
    /// This order is observable as maps are exposed both directly
    /// and indirectly via `Ord for `[`AlgebraicValue`].
    /// The latter lets us observe that e.g., `{ a: 42 } < { b: 42 }`.
    /// However, we cannot observe any difference between `{ a: 0, b: 0 }` and `{ b: 0, a: 0 }`,
    /// as the natural order is used as opposed to insertion order.
    /// Where insertion order is relevant,
    /// a [`BuiltinValue::Array`] with `(key, value)` pairs can be used instead.
    Map { val: MapValue },
}

impl BuiltinValue {
    /// Returns the byte string `v` as a [`BuiltinValue`].
    #[allow(non_snake_case)]
    pub const fn Bytes(v: Vec<u8>) -> Self {
        Self::Array { val: ArrayValue::U8(v) }
    }

    /// Returns `self` as a borrowed byte string, if applicable.
    pub fn as_bytes(&self) -> Option<&Vec<u8>> {
        match self {
            BuiltinValue::Array { val: ArrayValue::U8(v) } => Some(v),
            _ => None,
        }
    }

    /// Converts `self` into a byte string, if applicable.
    pub fn into_bytes(self) -> Result<Vec<u8>, Self> {
        match self {
            BuiltinValue::Array { val: ArrayValue::U8(v) } => Ok(v),
            _ => Err(self),
        }
    }
}

impl crate::Value for BuiltinValue {
    type Type = BuiltinType;
}
