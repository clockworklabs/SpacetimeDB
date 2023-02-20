// pub mod encoding;
pub mod de;
pub mod satn;
pub mod ser;
use std::collections::BTreeMap;

use crate::builtin_value::{F32, F64};
use crate::{AlgebraicType, BuiltinValue, ProductValue, SumValue};
use enum_as_inner::EnumAsInner;

#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AlgebraicValue {
    Sum(SumValue),
    Product(ProductValue),
    Builtin(BuiltinValue),
}

impl AlgebraicValue {
    #[inline]
    pub fn as_bool(&self) -> Option<&bool> {
        self.as_builtin()?.as_bool()
    }
    #[inline]
    pub fn as_i8(&self) -> Option<&i8> {
        self.as_builtin()?.as_i8()
    }
    #[inline]
    pub fn as_u8(&self) -> Option<&u8> {
        self.as_builtin()?.as_u8()
    }
    #[inline]
    pub fn as_i16(&self) -> Option<&i16> {
        self.as_builtin()?.as_i16()
    }
    #[inline]
    pub fn as_u16(&self) -> Option<&u16> {
        self.as_builtin()?.as_u16()
    }
    #[inline]
    pub fn as_i32(&self) -> Option<&i32> {
        self.as_builtin()?.as_i32()
    }
    #[inline]
    pub fn as_u32(&self) -> Option<&u32> {
        self.as_builtin()?.as_u32()
    }
    #[inline]
    pub fn as_i64(&self) -> Option<&i64> {
        self.as_builtin()?.as_i64()
    }
    #[inline]
    pub fn as_u64(&self) -> Option<&u64> {
        self.as_builtin()?.as_u64()
    }
    #[inline]
    pub fn as_i128(&self) -> Option<&i128> {
        self.as_builtin()?.as_i128()
    }
    #[inline]
    pub fn as_u128(&self) -> Option<&u128> {
        self.as_builtin()?.as_u128()
    }
    #[inline]
    pub fn as_f32(&self) -> Option<&F32> {
        self.as_builtin()?.as_f32()
    }
    #[inline]
    pub fn as_f64(&self) -> Option<&F64> {
        self.as_builtin()?.as_f64()
    }
    #[inline]
    pub fn as_string(&self) -> Option<&String> {
        self.as_builtin()?.as_string()
    }
    #[inline]
    pub fn as_bytes(&self) -> Option<&Vec<u8>> {
        self.as_builtin()?.as_bytes()
    }
    #[inline]
    pub fn as_array(&self) -> Option<&Vec<AlgebraicValue>> {
        self.as_builtin()?.as_array()
    }
    #[inline]
    pub fn as_map(&self) -> Option<&BTreeMap<AlgebraicValue, AlgebraicValue>> {
        self.as_builtin()?.as_map()
    }

    #[inline]
    pub fn into_bool(self) -> Result<bool, Self> {
        self.into_builtin()?.into_bool().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_i8(self) -> Result<i8, Self> {
        self.into_builtin()?.into_i8().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_u8(self) -> Result<u8, Self> {
        self.into_builtin()?.into_u8().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_i16(self) -> Result<i16, Self> {
        self.into_builtin()?.into_i16().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_u16(self) -> Result<u16, Self> {
        self.into_builtin()?.into_u16().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_i32(self) -> Result<i32, Self> {
        self.into_builtin()?.into_i32().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_u32(self) -> Result<u32, Self> {
        self.into_builtin()?.into_u32().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_i64(self) -> Result<i64, Self> {
        self.into_builtin()?.into_i64().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_u64(self) -> Result<u64, Self> {
        self.into_builtin()?.into_u64().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_i128(self) -> Result<i128, Self> {
        self.into_builtin()?.into_i128().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_u128(self) -> Result<u128, Self> {
        self.into_builtin()?.into_u128().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_f32(self) -> Result<F32, Self> {
        self.into_builtin()?.into_f32().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_f64(self) -> Result<F64, Self> {
        self.into_builtin()?.into_f64().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_string(self) -> Result<String, Self> {
        self.into_builtin()?.into_string().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_bytes(self) -> Result<Vec<u8>, Self> {
        self.into_builtin()?.into_bytes().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_array(self) -> Result<Vec<AlgebraicValue>, Self> {
        self.into_builtin()?.into_array().map_err(Self::Builtin)
    }
    #[inline]
    pub fn into_map(self) -> Result<BTreeMap<AlgebraicValue, AlgebraicValue>, Self> {
        self.into_builtin()?.into_map().map_err(Self::Builtin)
    }

    #[allow(non_snake_case)]
    #[inline]
    pub fn Bool(v: bool) -> Self {
        Self::Builtin(BuiltinValue::Bool(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn I8(v: i8) -> Self {
        Self::Builtin(BuiltinValue::I8(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn U8(v: u8) -> Self {
        Self::Builtin(BuiltinValue::U8(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn I16(v: i16) -> Self {
        Self::Builtin(BuiltinValue::I16(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn U16(v: u16) -> Self {
        Self::Builtin(BuiltinValue::U16(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn I32(v: i32) -> Self {
        Self::Builtin(BuiltinValue::I32(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn U32(v: u32) -> Self {
        Self::Builtin(BuiltinValue::U32(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn I64(v: i64) -> Self {
        Self::Builtin(BuiltinValue::I64(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn U64(v: u64) -> Self {
        Self::Builtin(BuiltinValue::U64(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn I128(v: i128) -> Self {
        Self::Builtin(BuiltinValue::I128(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn U128(v: u128) -> Self {
        Self::Builtin(BuiltinValue::U128(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn F32(v: F32) -> Self {
        Self::Builtin(BuiltinValue::F32(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn F64(v: F64) -> Self {
        Self::Builtin(BuiltinValue::F64(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn String(v: String) -> Self {
        Self::Builtin(BuiltinValue::String(v))
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn Bytes(v: Vec<u8>) -> Self {
        Self::Builtin(BuiltinValue::Bytes(v))
    }
}

macro_rules! impl_from {
    ($var:ident, $ty:ty) => {
        impl From<$ty> for AlgebraicValue {
            fn from(v: $ty) -> Self {
                AlgebraicValue::Builtin(BuiltinValue::$var(v.into()))
            }
        }
    };
}

impl_from!(Bool, bool);
impl_from!(I8, i8);
impl_from!(U8, u8);
impl_from!(I16, i16);
impl_from!(U16, u16);
impl_from!(I32, i32);
impl_from!(U32, u32);
impl_from!(I64, i64);
impl_from!(U64, u64);
impl_from!(I128, i128);
impl_from!(U128, u128);
impl_from!(F32, f32);
impl_from!(F64, f64);
impl_from!(String, String);
impl_from!(String, &str);

impl From<BuiltinValue> for AlgebraicValue {
    fn from(value: BuiltinValue) -> Self {
        AlgebraicValue::Builtin(value)
    }
}

impl crate::Value for AlgebraicValue {
    type Type = AlgebraicType;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::satn::Satn;
    use crate::{
        AlgebraicType, AlgebraicValue, BuiltinType, BuiltinValue, MapType, ProductType, ProductTypeElement,
        ProductValue, SumValue, TypeInSpace, Typespace, ValueWithType,
    };

    fn in_space<'a, T: crate::Value>(ts: &'a Typespace, ty: &'a T::Type, val: &'a T) -> ValueWithType<'a, T> {
        TypeInSpace::new(ts, ty).with_value(val)
    }

    #[test]
    fn unit() {
        let val = AlgebraicValue::Product(ProductValue { elements: vec![] });
        let unit = AlgebraicType::Product(ProductType::new(vec![]));
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &unit, &val).to_satn(), "()");
    }

    #[test]
    fn product_value() {
        let product_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement::new(
            AlgebraicType::Builtin(BuiltinType::I32),
            Some("foo".into()),
        )]));
        let typespace = Typespace::new(vec![]);
        let product_value = AlgebraicValue::Product(ProductValue {
            elements: vec![AlgebraicValue::Builtin(BuiltinValue::I32(42))],
        });
        assert_eq!(
            "(foo = 42)",
            in_space(&typespace, &product_type, &product_value).to_satn(),
        );
    }

    #[test]
    fn option_some() {
        let never = AlgebraicType::make_never_type();
        let option = AlgebraicType::make_option_type(never);
        let sum_value = AlgebraicValue::Sum(SumValue {
            tag: 1,
            value: Box::new(AlgebraicValue::Product(ProductValue { elements: Vec::new() })),
        });
        let typespace = Typespace::new(vec![]);
        assert_eq!("(none = ())", in_space(&typespace, &option, &sum_value).to_satn(),);
    }

    #[test]
    fn primitive() {
        let u8 = AlgebraicType::Builtin(BuiltinType::U8);
        let value = AlgebraicValue::Builtin(BuiltinValue::U8(255));
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &u8, &value).to_satn(), "255");
    }

    #[test]
    fn array() {
        let array = AlgebraicType::Builtin(BuiltinType::Array {
            ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        });
        let value = AlgebraicValue::Builtin(BuiltinValue::Array { val: Vec::new() });
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &array, &value).to_satn(), "[]");
    }

    #[test]
    fn array_of_values() {
        let array = AlgebraicType::Builtin(BuiltinType::Array {
            ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        });
        let value = AlgebraicValue::Builtin(BuiltinValue::Array {
            val: vec![AlgebraicValue::Builtin(BuiltinValue::U8(3))],
        });
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &array, &value).to_satn(), "[3]");
    }

    #[test]
    fn map() {
        let map = AlgebraicType::Builtin(BuiltinType::Map(MapType {
            key_ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
            ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        }));
        let value = AlgebraicValue::Builtin(BuiltinValue::Map { val: BTreeMap::new() });
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &map, &value).to_satn(), "[:]");
    }

    #[test]
    fn map_of_values() {
        let map = AlgebraicType::Builtin(BuiltinType::Map(MapType {
            key_ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
            ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        }));
        let mut value = BTreeMap::<AlgebraicValue, AlgebraicValue>::new();
        value.insert(
            AlgebraicValue::Builtin(BuiltinValue::U8(2)),
            AlgebraicValue::Builtin(BuiltinValue::U8(3)),
        );
        let value = AlgebraicValue::Builtin(BuiltinValue::Map { val: value });
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &map, &value).to_satn(), "[2: 3]");
    }
}
