pub mod de;
pub mod ser;
use std::collections::BTreeMap;
use std::ops::{Bound, Deref, RangeBounds};

use crate::builtin_value::{F32, F64};
use crate::{static_assert_size, AlgebraicType, ArrayValue, BuiltinType, BuiltinValue, ProductValue, SumValue};
use enum_as_inner::EnumAsInner;

/// A value in SATS typed at some [`AlgebraicType`].
///
/// Values are type erased, so they do not store their type.
/// This is important mainly for space efficiency,
/// including network latency and bandwidth.
///
/// These are only values and not expressions.
/// That is, they are canonical and cannot be simplified further by some evaluation.
/// So forms like `42 + 24` are not represented in an `AlgebraicValue`.
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
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
    /// A builtin value that has a builtin type.
    Builtin(BuiltinValue),
}

static_assert_size!(AlgebraicValue, 32);

#[allow(non_snake_case)]
impl AlgebraicValue {
    /// Interpret the value as a `bool` or `None` if it isn't a `bool` value.
    #[inline]
    pub fn as_bool(&self) -> Option<&bool> {
        self.as_builtin()?.as_bool()
    }

    /// Interpret the value as an `i8` or `None` if it isn't a `i8` value.
    #[inline]
    pub fn as_i8(&self) -> Option<&i8> {
        self.as_builtin()?.as_i8()
    }

    /// Interpret the value as a `u8` or `None` if it isn't a `u8` value.
    #[inline]
    pub fn as_u8(&self) -> Option<&u8> {
        self.as_builtin()?.as_u8()
    }

    /// Interpret the value as an `i16` or `None` if it isn't an `i16` value.
    #[inline]
    pub fn as_i16(&self) -> Option<&i16> {
        self.as_builtin()?.as_i16()
    }

    /// Interpret the value as a `u16` or `None` if it isn't a `u16` value.
    #[inline]
    pub fn as_u16(&self) -> Option<&u16> {
        self.as_builtin()?.as_u16()
    }

    /// Interpret the value as an `i32` or `None` if it isn't an `i32` value.
    #[inline]
    pub fn as_i32(&self) -> Option<&i32> {
        self.as_builtin()?.as_i32()
    }

    /// Interpret the value as a `u32` or `None` if it isn't a `u32` value.
    #[inline]
    pub fn as_u32(&self) -> Option<&u32> {
        self.as_builtin()?.as_u32()
    }

    /// Interpret the value as an `i64` or `None` if it isn't an `i64` value.
    #[inline]
    pub fn as_i64(&self) -> Option<&i64> {
        self.as_builtin()?.as_i64()
    }

    /// Interpret the value as a `u64` or `None` if it isn't a `u64` value.
    #[inline]
    pub fn as_u64(&self) -> Option<&u64> {
        self.as_builtin()?.as_u64()
    }

    /// Interpret the value as an `i128` or `None` if it isn't an `i128` value.
    #[inline]
    pub fn as_i128(&self) -> Option<&i128> {
        self.as_builtin()?.as_i128()
    }

    /// Interpret the value as a `u128` or `None` if it isn't a `u128` value.
    #[inline]
    pub fn as_u128(&self) -> Option<&u128> {
        self.as_builtin()?.as_u128()
    }

    /// Interpret the value as a `f32` or `None` if it isn't a `f32` value.
    #[inline]
    pub fn as_f32(&self) -> Option<&F32> {
        self.as_builtin()?.as_f32()
    }

    /// Interpret the value as a `f64` or `None` if it isn't a `f64` value.
    #[inline]
    pub fn as_f64(&self) -> Option<&F64> {
        self.as_builtin()?.as_f64()
    }

    /// Interpret the value as a `String` or `None` if it isn't a `String` value.
    #[inline]
    pub fn as_string(&self) -> Option<&str> {
        self.as_builtin()?.as_string().map(|x| x.deref())
    }

    /// Interpret the value as a byte slice or `None` if it isn't a byte slice.
    #[inline]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.as_builtin()?.as_bytes()
    }

    /// Interpret the value as an `ArrayValue` or `None` if it isn't an `ArrayValue` value.
    #[inline]
    pub fn as_array(&self) -> Option<&ArrayValue> {
        self.as_builtin()?.as_array()
    }

    /// Interpret the value as a map or `None` if it isn't a map value.
    #[inline]
    pub fn as_map(&self) -> Option<&BTreeMap<Self, Self>> {
        self.as_builtin()?.as_map()
    }

    /// Convert the value into a `bool` or `Err(self)` if it isn't a `bool` value.
    #[inline]
    pub fn into_bool(self) -> Result<bool, Self> {
        self.into_builtin()?.into_bool().map_err(Self::Builtin)
    }

    /// Convert the value into an `i8` or `Err(self)` if it isn't an `i8` value.
    #[inline]
    pub fn into_i8(self) -> Result<i8, Self> {
        self.into_builtin()?.into_i8().map_err(Self::Builtin)
    }

    /// Convert the value into a `u8` or `Err(self)` if it isn't a `u8` value.
    #[inline]
    pub fn into_u8(self) -> Result<u8, Self> {
        self.into_builtin()?.into_u8().map_err(Self::Builtin)
    }

    /// Convert the value into an `i16` or `Err(self)` if it isn't an `i16` value.
    #[inline]
    pub fn into_i16(self) -> Result<i16, Self> {
        self.into_builtin()?.into_i16().map_err(Self::Builtin)
    }

    /// Convert the value into a `u16` or `Err(self)` if it isn't a `u16` value.
    #[inline]
    pub fn into_u16(self) -> Result<u16, Self> {
        self.into_builtin()?.into_u16().map_err(Self::Builtin)
    }

    /// Convert the value into an `i32` or `Err(self)` if it isn't an `i32` value.
    #[inline]
    pub fn into_i32(self) -> Result<i32, Self> {
        self.into_builtin()?.into_i32().map_err(Self::Builtin)
    }

    /// Convert the value into a `u32` or `Err(self)` if it isn't a `u32` value.
    #[inline]
    pub fn into_u32(self) -> Result<u32, Self> {
        self.into_builtin()?.into_u32().map_err(Self::Builtin)
    }

    /// Convert the value into an `i64` or `Err(self)` if it isn't an `i64` value.
    #[inline]
    pub fn into_i64(self) -> Result<i64, Self> {
        self.into_builtin()?.into_i64().map_err(Self::Builtin)
    }

    /// Convert the value into a `u64` or `Err(self)` if it isn't a `u64` value.
    #[inline]
    pub fn into_u64(self) -> Result<u64, Self> {
        self.into_builtin()?.into_u64().map_err(Self::Builtin)
    }

    /// Convert the value into an `i128` or `Err(self)` if it isn't an `i128` value.
    #[inline]
    pub fn into_i128(self) -> Result<i128, Self> {
        self.into_builtin()?.into_i128().map_err(Self::Builtin)
    }

    /// Convert the value into a `u128` or `Err(self)` if it isn't a `u128` value.
    #[inline]
    pub fn into_u128(self) -> Result<u128, Self> {
        self.into_builtin()?.into_u128().map_err(Self::Builtin)
    }

    /// Convert the value into a `f32` or `Err(self)` if it isn't a `f32` value.
    #[inline]
    pub fn into_f32(self) -> Result<F32, Self> {
        self.into_builtin()?.into_f32().map_err(Self::Builtin)
    }

    /// Convert the value into a `f64` or `Err(self)` if it isn't a `f64` value.
    #[inline]
    pub fn into_f64(self) -> Result<F64, Self> {
        self.into_builtin()?.into_f64().map_err(Self::Builtin)
    }

    /// Convert the value into a string or `Err(self)` if it isn't a string value.
    #[inline]
    pub fn into_string(self) -> Result<Box<str>, Self> {
        self.into_builtin()?.into_string().map_err(Self::Builtin)
    }

    /// Convert the value into a `Box<[u8]>`
    /// or `Err(self)` if it isn't a `Box<[u8]>` value.
    #[inline]
    pub fn into_bytes(self) -> Result<Box<[u8]>, Self> {
        self.into_builtin()?.into_bytes().map_err(Self::Builtin)
    }

    /// Convert the value into an [`ArrayValue`] or `Err(self)` if it isn't an [`ArrayValue`] value.
    #[inline]
    pub fn into_array(self) -> Result<ArrayValue, Self> {
        self.into_builtin()?.into_array().map_err(Self::Builtin)
    }

    /// Convert the value into a map or `Err(self)` if it isn't a map value.
    #[inline]
    pub fn into_map(self) -> Result<BTreeMap<Self, Self>, Self> {
        self.into_builtin()?.into_map().map_err(Self::Builtin)
    }

    /// The canonical unit value defined as the nullary product value `()`.
    ///
    /// The type of `UNIT` is `()`.
    pub fn unit() -> Self {
        Self::product([].into())
    }

    /// Returns an [`AlgebraicValue`] representing `v: bool`.
    #[inline]
    pub const fn Bool(v: bool) -> Self {
        Self::Builtin(BuiltinValue::Bool(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: i8`.
    #[inline]
    pub const fn I8(v: i8) -> Self {
        Self::Builtin(BuiltinValue::I8(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: u8`.
    #[inline]
    pub const fn U8(v: u8) -> Self {
        Self::Builtin(BuiltinValue::U8(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: i16`.
    #[inline]
    pub const fn I16(v: i16) -> Self {
        Self::Builtin(BuiltinValue::I16(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: u16`.
    #[inline]
    pub const fn U16(v: u16) -> Self {
        Self::Builtin(BuiltinValue::U16(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: i32`.
    #[inline]
    pub const fn I32(v: i32) -> Self {
        Self::Builtin(BuiltinValue::I32(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: u32`.
    #[inline]
    pub const fn U32(v: u32) -> Self {
        Self::Builtin(BuiltinValue::U32(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: i64`.
    #[inline]
    pub const fn I64(v: i64) -> Self {
        Self::Builtin(BuiltinValue::I64(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: u64`.
    #[inline]
    pub const fn U64(v: u64) -> Self {
        Self::Builtin(BuiltinValue::U64(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: i128`.
    #[inline]
    pub const fn I128(v: i128) -> Self {
        Self::Builtin(BuiltinValue::I128(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: u128`.
    #[inline]
    pub const fn U128(v: u128) -> Self {
        Self::Builtin(BuiltinValue::U128(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: f32`.
    #[inline]
    pub const fn F32(v: F32) -> Self {
        Self::Builtin(BuiltinValue::F32(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: f64`.
    #[inline]
    pub const fn F64(v: F64) -> Self {
        Self::Builtin(BuiltinValue::F64(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: String`.
    #[inline]
    pub const fn String(v: Box<str>) -> Self {
        Self::Builtin(BuiltinValue::String(v))
    }

    /// Returns an [`AlgebraicValue`] representing `v: Vec<u8>`.
    #[inline]
    pub const fn Bytes(v: Box<[u8]>) -> Self {
        Self::Builtin(BuiltinValue::Bytes(v))
    }

    /// Returns an [`AlgebraicValue`] for a `val` which can be converted into an [`ArrayValue`].
    #[inline]
    pub fn ArrayOf(val: impl Into<ArrayValue>) -> Self {
        Self::Builtin(BuiltinValue::Array { val: val.into() })
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
    pub const fn product(elements: Box<[Self]>) -> Self {
        Self::Product(ProductValue { elements })
    }

    /// Returns an [`AlgebraicValue`] representing a map value defined by the given `map`.
    pub const fn map(map: BTreeMap<Self, Self>) -> Self {
        Self::Builtin(BuiltinValue::Map { val: map })
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
        AlgebraicType::product([x.value.type_of().into()].into())
    }

    /// Returns the [`AlgebraicType`] of the product value `x`.
    pub(crate) fn type_of_product(x: &ProductValue) -> AlgebraicType {
        AlgebraicType::product(x.elements.iter().map(|x| x.type_of().into()).collect())
    }

    /// Returns the [`AlgebraicType`] of the map with key type `k` and value type `v`.
    pub(crate) fn type_of_map(val: &BTreeMap<Self, Self>) -> AlgebraicType {
        AlgebraicType::product(if let Some((k, v)) = val.first_key_value() {
            [k.type_of().into(), v.type_of().into()].into()
        } else {
            // TODO(centril): What is the motivation for this?
            //   I think this requires a soundness argument.
            //   I could see that it is OK with the argument that this is an empty map
            //   under the requirement that we cannot insert elements into the map.
            vec![AlgebraicType::never().into(); 2].into()
        })
    }

    /// Infer the [`AlgebraicType`] of an [`AlgebraicValue`].
    pub fn type_of(&self) -> AlgebraicType {
        // TODO: What are the types of empty arrays/maps/sums?
        match self {
            AlgebraicValue::Sum(x) => Self::type_of_sum(x),
            AlgebraicValue::Product(x) => Self::type_of_product(x),
            AlgebraicValue::Builtin(x) => match x {
                BuiltinValue::Bool(_) => AlgebraicType::Bool,
                BuiltinValue::I8(_) => AlgebraicType::I8,
                BuiltinValue::U8(_) => AlgebraicType::U8,
                BuiltinValue::I16(_) => AlgebraicType::I16,
                BuiltinValue::U16(_) => AlgebraicType::U16,
                BuiltinValue::I32(_) => AlgebraicType::I32,
                BuiltinValue::U32(_) => AlgebraicType::U32,
                BuiltinValue::I64(_) => AlgebraicType::I64,
                BuiltinValue::U64(_) => AlgebraicType::U64,
                BuiltinValue::I128(_) => AlgebraicType::I128,
                BuiltinValue::U128(_) => AlgebraicType::U128,
                BuiltinValue::F32(_) => AlgebraicType::F32,
                BuiltinValue::F64(_) => AlgebraicType::F64,
                BuiltinValue::String(_) => AlgebraicType::String,
                BuiltinValue::Array { val } => AlgebraicType::Builtin(BuiltinType::Array(val.type_of())),
                BuiltinValue::Map { val } => Self::type_of_map(val),
            },
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
    use crate::{
        AlgebraicType, AlgebraicValue, ArrayValue, ProductTypeElement, Typespace, ValueWithType, WithTypespace,
    };

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
        let product_type = AlgebraicType::product([ProductTypeElement::new_named(AlgebraicType::I32, "foo")].into());
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
        let value = AlgebraicValue::ArrayOf(ArrayValue::Sum([].into()));
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &array, &value).to_satn(), "[]");
    }

    #[test]
    fn array_of_values() {
        let array = AlgebraicType::array(AlgebraicType::U8);
        let value = AlgebraicValue::ArrayOf([3u8]);
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &array, &value).to_satn(), "[3]");
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
