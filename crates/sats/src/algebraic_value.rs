pub mod de;
pub mod ser;

use crate::{AlgebraicType, ArrayValue, MapValue, ProductValue, SumValue};
use derive_more::From;
use enum_as_inner::EnumAsInner;
use std::ops::{Bound, RangeBounds};

/// Totally ordered [`f32`] allowing all IEEE-754 floating point values.
pub type F32 = decorum::Total<f32>;

/// Totally ordered [`f64`] allowing all IEEE-754 floating point values.
pub type F64 = decorum::Total<f64>;

/// A value in SATS typed at some [`AlgebraicType`].
///
/// Values are type erased, so they do not store their type.
/// This is important mainly for space efficiency,
/// including network latency and bandwidth.
///
/// These are only values and not expressions.
/// That is, they are canonical and cannot be simplified further by some evaluation.
/// So forms like `42 + 24` are not represented in an `AlgebraicValue`.
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, From)]
pub enum AlgebraicValue {
    /// A structural sum value.
    ///
    /// Given a sum type `{ N_0(T_0), N_1(T_1), ..., N_n(T_n) }`
    /// where `N_i` denotes a variant name
    /// and where `T_i` denotes the type the variant stores,
    /// a sum value makes a specific choice as to the variant.
    /// So for example, we might chose `N_1(T_1)`
    /// and represent this choice with `(1, v)` where `v` is a value of type `T_1`.
    Sum(SumValue),
    /// A structural product value.
    ///
    /// Given a product type `{ N_0: T_0, N_1: T_1, ..., N_n: T_n }`
    /// where `N_i` denotes a field / element name
    /// and where `T_i` denotes the type the field stores,
    /// a product value stores a value `v_i` of type `T_i` for each field `N_i`.
    Product(ProductValue),
    /// A homogeneous array of `AlgebraicValue`s.
    /// The array has the type [`AlgebraicType::Array(elem_ty)`].
    ///
    /// The contained values are stored packed in a representation appropriate for their type.
    /// See [`ArrayValue`] for details on the representation.
    Array(ArrayValue),
    /// An ordered map value of `key: AlgebraicValue`s mapped to `value: AlgebraicValue`s.
    /// Each `key` must be of the same [`AlgebraicType`] as all the others
    /// and the same applies to each `value`.
    /// A map as a whole has the type [`AlgebraicType::Map(key_ty, val_ty)`].
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
    /// a [`AlgebraicValue::Array`] with `(key, value)` pairs can be used instead.
    ///
    /// We box the `MapValue` to reduce size
    /// and because we assume that map values will be uncommon.
    Map(MapValue),
    /// A [`bool`] value of type [`AlgebraicType::Bool`].
    Bool(bool),
    /// An [`i8`] value of type [`AlgebraicType::I8`].
    I8(i8),
    /// A [`u8`] value of type [`AlgebraicType::U8`].
    U8(u8),
    /// An [`i16`] value of type [`AlgebraicType::I16`].
    I16(i16),
    /// A [`u16`] value of type [`AlgebraicType::U16`].
    U16(u16),
    /// An [`i32`] value of type [`AlgebraicType::I32`].
    I32(i32),
    /// A [`u32`] value of type [`AlgebraicType::U32`].
    U32(u32),
    /// An [`i64`] value of type [`AlgebraicType::I64`].
    I64(i64),
    /// A [`u64`] value of type [`AlgebraicType::U64`].
    U64(u64),
    /// An [`i128`] value of type [`AlgebraicType::I128`].
    ///
    /// We box these up as they allow us to shrink `AlgebraicValue`.
    I128(i128),
    /// A [`u128`] value of type [`AlgebraicType::U128`].
    ///
    /// We box these up as they allow us to shrink `AlgebraicValue`.
    U128(u128),
    /// A totally ordered [`F32`] value of type [`AlgebraicType::F32`].
    ///
    /// All floating point values defined in IEEE-754 are supported.
    /// However, unlike the primitive [`f32`], a [total order] is established.
    ///
    /// [total order]: https://docs.rs/decorum/0.3.1/decorum/#total-ordering
    F32(F32),
    /// A totally ordered [`F64`] value of type [`AlgebraicType::F64`].
    ///
    /// All floating point values defined in IEEE-754 are supported.
    /// However, unlike the primitive [`f64`], a [total order] is established.
    ///
    /// [total order]: https://docs.rs/decorum/0.3.1/decorum/#total-ordering
    F64(F64),
    /// A UTF-8 string value of type [`AlgebraicType::String`].
    ///
    /// Uses Rust's standard representation of strings.
    String(String),
}

#[allow(non_snake_case)]
impl AlgebraicValue {
    /// Interpret the value as a byte slice or `None` if it isn't a byte slice.
    #[inline]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Array(ArrayValue::U8(a)) => Some(a),
            _ => None,
        }
    }

    /// The canonical unit value defined as the nullary product value `()`.
    ///
    /// The type of `UNIT` is `()`.
    pub fn unit() -> Self {
        Self::product([].into())
    }

    /// Returns an [`AlgebraicValue`] representing `v: Vec<u8>`.
    #[inline]
    pub const fn Bytes(v: Vec<u8>) -> Self {
        Self::Array(ArrayValue::U8(v))
    }

    /// Converts `self` into a byte string, if applicable.
    pub fn into_bytes(self) -> Result<Vec<u8>, Self> {
        match self {
            Self::Array(ArrayValue::U8(v)) => Ok(v),
            _ => Err(self),
        }
    }

    /// Returns an [`AlgebraicValue`] for `some: v`.
    ///
    /// The `some` variant is assigned the tag `0`.
    #[inline]
    pub fn OptionSome(v: Self) -> Self {
        Self::sum(0, v)
    }

    /// Returns an [`AlgebraicValue`] for `none`.
    ///
    /// The `none` variant is assigned the tag `1`.
    #[inline]
    pub fn OptionNone() -> Self {
        Self::sum(1, Self::unit())
    }

    /// Returns an [`AlgebraicValue`] representing a sum value with `tag` and `value`.
    pub fn sum(tag: u8, value: Self) -> Self {
        let value = Box::new(value);
        Self::Sum(SumValue { tag, value })
    }

    /// Returns an [`AlgebraicValue`] representing a product value with the given `elements`.
    pub const fn product(elements: Vec<Self>) -> Self {
        Self::Product(ProductValue { elements })
    }

    /// Returns an [`AlgebraicValue`] representing a map value defined by the given `map`.
    pub fn map(map: MapValue) -> Self {
        Self::Map(map)
    }

    /// Returns the [`AlgebraicType`] of the sum value `x`.
    pub(crate) fn type_of_sum(x: &SumValue) -> AlgebraicType {
        // TODO(centril, #104): This is unsound!
        //
        //   The type of a sum value must be a sum type and *not* a product type.
        //   Suppose `x.tag` is for the variant `VarName(VarType)`.
        //   Then `VarType` is *not* the same type as `{ VarName(VarType) | r }`
        //   where `r` represents a polymorphic variants compontent.
        //
        //   To assign this a correct type we either have to store the type with the value
        //   or alternatively, we must have polymorphic variants (see row polymorphism)
        //   *and* derive the correct variant name.
        AlgebraicType::product([x.value.type_of()])
    }

    /// Returns the [`AlgebraicType`] of the product value `x`.
    pub(crate) fn type_of_product(x: &ProductValue) -> AlgebraicType {
        AlgebraicType::product(x.elements.iter().map(|x| x.type_of().into()).collect::<Vec<_>>())
    }

    /// Returns the [`AlgebraicType`] of the map with key type `k` and value type `v`.
    pub(crate) fn type_of_map(val: &MapValue) -> AlgebraicType {
        AlgebraicType::product(if let Some((k, v)) = val.first_key_value() {
            [k.type_of(), v.type_of()]
        } else {
            // TODO(centril): What is the motivation for this?
            //   I think this requires a soundness argument.
            //   I could see that it is OK with the argument that this is an empty map
            //   under the requirement that we cannot insert elements into the map.
            [AlgebraicType::never(), AlgebraicType::never()]
        })
    }

    /// Infer the [`AlgebraicType`] of an [`AlgebraicValue`].
    pub fn type_of(&self) -> AlgebraicType {
        // TODO: What are the types of empty arrays/maps/sums?
        match self {
            Self::Sum(x) => Self::type_of_sum(x),
            Self::Product(x) => Self::type_of_product(x),
            Self::Array(x) => x.type_of().into(),
            Self::Map(x) => Self::type_of_map(x),
            Self::Bool(_) => AlgebraicType::Bool,
            Self::I8(_) => AlgebraicType::I8,
            Self::U8(_) => AlgebraicType::U8,
            Self::I16(_) => AlgebraicType::I16,
            Self::U16(_) => AlgebraicType::U16,
            Self::I32(_) => AlgebraicType::I32,
            Self::U32(_) => AlgebraicType::U32,
            Self::I64(_) => AlgebraicType::I64,
            Self::U64(_) => AlgebraicType::U64,
            Self::I128(_) => AlgebraicType::I128,
            Self::U128(_) => AlgebraicType::U128,
            Self::F32(_) => AlgebraicType::F32,
            Self::F64(_) => AlgebraicType::F64,
            Self::String(_) => AlgebraicType::String,
        }
    }

    /// Returns whether this value represents a numeric zero.
    ///
    /// Can only be true where the type is numeric.
    pub fn is_numeric_zero(&self) -> bool {
        match *self {
            Self::I8(x) => x == 0,
            Self::U8(x) => x == 0,
            Self::I16(x) => x == 0,
            Self::U16(x) => x == 0,
            Self::I32(x) => x == 0,
            Self::U32(x) => x == 0,
            Self::I64(x) => x == 0,
            Self::U64(x) => x == 0,
            Self::I128(x) => x == 0,
            Self::U128(x) => x == 0,
            Self::F32(x) => x == 0.0,
            Self::F64(x) => x == 0.0,
            _ => false,
        }
    }

    /// Converts `sequence_value` to an appropriate `AlgebraicValue` based on `ty`.
    /// Truncates the `sequence_value` to fit `ty`.
    ///
    /// Panics if `ty` is not an integer type.
    pub fn from_sequence_value(ty: &AlgebraicType, sequence_value: i128) -> Self {
        match *ty {
            AlgebraicType::I8 => (sequence_value as i8).into(),
            AlgebraicType::U8 => (sequence_value as u8).into(),
            AlgebraicType::I16 => (sequence_value as i16).into(),
            AlgebraicType::U16 => (sequence_value as u16).into(),
            AlgebraicType::I32 => (sequence_value as i32).into(),
            AlgebraicType::U32 => (sequence_value as u32).into(),
            AlgebraicType::I64 => (sequence_value as i64).into(),
            AlgebraicType::U64 => (sequence_value as u64).into(),
            AlgebraicType::I128 => sequence_value.into(),
            AlgebraicType::U128 => (sequence_value as u128).into(),
            _ => panic!("`{ty:?}` is not an integer type"),
        }
    }
}

impl<T: Into<AlgebraicValue>> From<Option<T>> for AlgebraicValue {
    fn from(value: Option<T>) -> Self {
        match value {
            None => AlgebraicValue::OptionNone(),
            Some(x) => AlgebraicValue::OptionSome(x.into()),
        }
    }
}

// An AlgebraicValue can be interpreted as a range containing a only the value itself.
// This is useful for BTrees where single key scans are still viewed range scans.
impl RangeBounds<AlgebraicValue> for AlgebraicValue {
    fn start_bound(&self) -> Bound<&AlgebraicValue> {
        Bound::Included(self)
    }
    fn end_bound(&self) -> Bound<&AlgebraicValue> {
        Bound::Included(self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::satn::Satn;
    use crate::{AlgebraicType, AlgebraicValue, ArrayValue, Typespace, ValueWithType, WithTypespace};

    fn in_space<'a, T: crate::Value>(ts: &'a Typespace, ty: &'a T::Type, val: &'a T) -> ValueWithType<'a, T> {
        WithTypespace::new(ts, ty).with_value(val)
    }

    #[test]
    fn unit() {
        let val = AlgebraicValue::unit();
        let unit = AlgebraicType::unit();
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &unit, &val).to_satn(), "()");
    }

    #[test]
    fn product_value() {
        let product_type = AlgebraicType::product([("foo", AlgebraicType::I32)]);
        let typespace = Typespace::new(vec![]);
        let product_value = AlgebraicValue::product([AlgebraicValue::I32(42)].into());
        assert_eq!(
            "(foo = 42)",
            in_space(&typespace, &product_type, &product_value).to_satn(),
        );
    }

    #[test]
    fn option_some() {
        let option = AlgebraicType::option(AlgebraicType::never());
        let sum_value = AlgebraicValue::OptionNone();
        let typespace = Typespace::new(vec![]);
        assert_eq!("(none = ())", in_space(&typespace, &option, &sum_value).to_satn(),);
    }

    #[test]
    fn primitive() {
        let u8 = AlgebraicType::U8;
        let value = AlgebraicValue::U8(255);
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &u8, &value).to_satn(), "255");
    }

    #[test]
    fn array() {
        let array = AlgebraicType::array(AlgebraicType::U8);
        let value = AlgebraicValue::Array(ArrayValue::Sum(Vec::new()));
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &array, &value).to_satn(), "[]");
    }

    #[test]
    fn array_of_values() {
        let array = AlgebraicType::array(AlgebraicType::U8);
        let value = AlgebraicValue::Array([3u8].into());
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &array, &value).to_satn(), "0x03");
    }

    #[test]
    fn map() {
        let map = AlgebraicType::map(AlgebraicType::U8, AlgebraicType::U8);
        let value = AlgebraicValue::map(BTreeMap::new());
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &map, &value).to_satn(), "[:]");
    }

    #[test]
    fn map_of_values() {
        let map = AlgebraicType::map(AlgebraicType::U8, AlgebraicType::U8);
        let mut val = BTreeMap::<AlgebraicValue, AlgebraicValue>::new();
        val.insert(AlgebraicValue::U8(2), AlgebraicValue::U8(3));
        let value = AlgebraicValue::map(val);
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &map, &value).to_satn(), "[2: 3]");
    }
}
