pub mod de;
pub mod ser;

use crate::{AlgebraicType, ArrayValue, ProductValue, SumValue};
use core::mem;
use core::ops::{Bound, RangeBounds};
use derive_more::From;
use enum_as_inner::EnumAsInner;

pub use ethnum::{i256, u256};

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
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, From)]
pub enum AlgebraicValue {
    /// The minimum value in the total ordering.
    /// Cannot be serialized and only exists to facilitate range index scans.
    /// This variant must always be first.
    Min,

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
    /// We pack these to shrink `AlgebraicValue`.
    I128(Packed<i128>),
    /// A [`u128`] value of type [`AlgebraicType::U128`].
    ///
    /// We pack these to to shrink `AlgebraicValue`.
    U128(Packed<u128>),
    /// An [`i256`] value of type [`AlgebraicType::I256`].
    ///
    /// We box these up to shrink `AlgebraicValue`.
    I256(Box<i256>),
    /// A [`u256`] value of type [`AlgebraicType::U256`].
    ///
    /// We pack these to shrink `AlgebraicValue`.
    U256(Box<u256>),
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
    String(Box<str>),

    /// The maximum value in the total ordering.
    /// Cannot be serialized and only exists to facilitate range index scans.
    /// This variant must always be last.
    Max,
}

/// Wraps `T` making the outer type packed with alignment 1.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(packed)]
pub struct Packed<T>(pub T);

impl<T> From<T> for Packed<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

#[allow(non_snake_case)]
impl AlgebraicValue {
    /// Extract the value and replace it with a dummy one that is cheap to make.
    pub fn take(&mut self) -> Self {
        mem::replace(self, Self::U8(0))
    }

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
        Self::product([])
    }

    /// Returns an [`AlgebraicValue`] representing `v: Box<[u8]>`.
    #[inline]
    pub const fn Bytes(v: Box<[u8]>) -> Self {
        Self::Array(ArrayValue::U8(v))
    }

    /// Converts `self` into a byte string, if applicable.
    pub fn into_bytes(self) -> Result<Box<[u8]>, Self> {
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

    /// Returns an [`AlgebraicValue`] representing a sum value with `tag` and empty [AlgebraicValue::product], that is
    /// valid for simple enums without payload.
    pub fn enum_simple(tag: u8) -> Self {
        let value = Box::new(AlgebraicValue::product(vec![]));
        Self::Sum(SumValue { tag, value })
    }

    /// Returns an [`AlgebraicValue`] representing a product value with the given `elements`.
    pub fn product(elements: impl Into<ProductValue>) -> Self {
        Self::Product(elements.into())
    }

    /// Returns the [`AlgebraicType`] of the product value `x`.
    pub(crate) fn type_of_product(x: &ProductValue) -> Option<AlgebraicType> {
        let mut elems = Vec::with_capacity(x.elements.len());
        for elem in &*x.elements {
            elems.push(elem.type_of()?.into());
        }
        Some(AlgebraicType::product(elems.into_boxed_slice()))
    }

    /// Infer the [`AlgebraicType`] of an [`AlgebraicValue`].
    ///
    /// This function is partial
    /// as type inference is not possible for `AlgebraicValue` in the case of sums.
    /// Thus the method only answers for the decidable subset.
    ///
    /// # A note on sums
    ///
    /// The type of a sum value must be a sum type and *not* a product type.
    /// Suppose `x.tag` is for the variant `VarName(VarType)`.
    /// Then `VarType` is *not* the same type as `{ VarName(VarType) | r }`
    /// where `r` represents a polymorphic variants component.
    ///
    /// To assign this a correct type we either have to store the type with the value
    /// r alternatively, we must have polymorphic variants (see row polymorphism)
    /// *and* derive the correct variant name.
    pub fn type_of(&self) -> Option<AlgebraicType> {
        match self {
            Self::Sum(_) => None,
            Self::Product(x) => Self::type_of_product(x),
            Self::Array(x) => x.type_of().map(Into::into),
            Self::Bool(_) => Some(AlgebraicType::Bool),
            Self::I8(_) => Some(AlgebraicType::I8),
            Self::U8(_) => Some(AlgebraicType::U8),
            Self::I16(_) => Some(AlgebraicType::I16),
            Self::U16(_) => Some(AlgebraicType::U16),
            Self::I32(_) => Some(AlgebraicType::I32),
            Self::U32(_) => Some(AlgebraicType::U32),
            Self::I64(_) => Some(AlgebraicType::I64),
            Self::U64(_) => Some(AlgebraicType::U64),
            Self::I128(_) => Some(AlgebraicType::I128),
            Self::U128(_) => Some(AlgebraicType::U128),
            Self::I256(_) => Some(AlgebraicType::I256),
            Self::U256(_) => Some(AlgebraicType::U256),
            Self::F32(_) => Some(AlgebraicType::F32),
            Self::F64(_) => Some(AlgebraicType::F64),
            Self::String(_) => Some(AlgebraicType::String),
            AlgebraicValue::Min | AlgebraicValue::Max => None,
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
            Self::I128(x) => x.0 == 0,
            Self::U128(x) => x.0 == 0,
            Self::I256(ref x) => **x == i256::ZERO,
            Self::U256(ref x) => **x == u256::ZERO,
            Self::F32(x) => x == 0.0,
            Self::F64(x) => x == 0.0,
            _ => false,
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

/// An AlgebraicValue can be interpreted as a range containing a only the value itself.
/// This is useful for BTrees where single key scans are still viewed range scans.
impl RangeBounds<AlgebraicValue> for &AlgebraicValue {
    fn start_bound(&self) -> Bound<&AlgebraicValue> {
        Bound::Included(self)
    }
    fn end_bound(&self) -> Bound<&AlgebraicValue> {
        Bound::Included(self)
    }
}

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
        let product_value = AlgebraicValue::product([AlgebraicValue::I32(42)]);
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
        let value = AlgebraicValue::Array(ArrayValue::Sum([].into()));
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
}
