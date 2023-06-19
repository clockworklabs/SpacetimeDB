pub mod de;
pub mod ser;
use std::collections::BTreeMap;

use crate::builtin_value::{F32, F64};
use crate::{
    AlgebraicType, ArrayValue, BuiltinType, BuiltinValue, ProductType, ProductTypeElement, ProductValue, SumValue,
};
use enum_as_inner::EnumAsInner;

#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AlgebraicValue {
    Sum(SumValue),
    Product(ProductValue),
    Builtin(BuiltinValue),
}

impl AlgebraicValue {
    pub const UNIT: Self = AlgebraicValue::Product(ProductValue { elements: Vec::new() });

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
    pub fn as_array(&self) -> Option<&ArrayValue> {
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
    pub fn into_array(self) -> Result<ArrayValue, Self> {
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
    #[allow(non_snake_case)]
    #[inline]
    pub fn ArrayOf<T: Into<ArrayValue>>(val: T) -> Self {
        Self::Builtin(BuiltinValue::Array { val: val.into() })
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn OptionSome(v: AlgebraicValue) -> Self {
        Self::Sum(SumValue {
            tag: 0,
            value: Box::new(v),
        })
    }
    #[allow(non_snake_case)]
    #[inline]
    pub fn OptionNone() -> Self {
        Self::Sum(SumValue {
            tag: 1,
            value: Box::new(AlgebraicValue::Product(ProductValue { elements: Vec::new() })),
        })
    }
    pub(crate) fn type_of_sum(x: &SumValue) -> AlgebraicType {
        AlgebraicType::Product(ProductType::new(vec![ProductTypeElement::new(x.value.type_of(), None)]))
    }
    pub(crate) fn type_of_product(x: &ProductValue) -> AlgebraicType {
        let ty = x.elements.iter().map(|x| ProductTypeElement::new(x.type_of(), None));

        AlgebraicType::Product(ProductType::new(ty.collect()))
    }
    pub(crate) fn type_of_map(val: &BTreeMap<AlgebraicValue, AlgebraicValue>) -> AlgebraicType {
        let ty = if let Some((k, v)) = val.first_key_value() {
            ProductType::new(vec![
                ProductTypeElement::new(k.type_of(), None),
                ProductTypeElement::new(v.type_of(), None),
            ])
        } else {
            let ty = ProductTypeElement::new(AlgebraicType::make_never_type(), None);
            ProductType::new(vec![ty.clone(), ty])
        };
        AlgebraicType::Product(ty)
    }

    /// Infer the [AlgebraicType] of [Self].
    pub fn type_of(&self) -> AlgebraicType {
        //todo: What are the types of empty arrays/maps/sums...
        match self {
            AlgebraicValue::Sum(x) => Self::type_of_sum(x),
            AlgebraicValue::Product(x) => Self::type_of_product(x),
            AlgebraicValue::Builtin(x) => match x {
                BuiltinValue::Bool(_) => BuiltinType::Bool.into(),
                BuiltinValue::I8(_) => BuiltinType::I8.into(),
                BuiltinValue::U8(_) => BuiltinType::U8.into(),
                BuiltinValue::I16(_) => BuiltinType::I16.into(),
                BuiltinValue::U16(_) => BuiltinType::U16.into(),
                BuiltinValue::I32(_) => BuiltinType::I32.into(),
                BuiltinValue::U32(_) => BuiltinType::U32.into(),
                BuiltinValue::I64(_) => BuiltinType::I64.into(),
                BuiltinValue::U64(_) => BuiltinType::U64.into(),
                BuiltinValue::I128(_) => BuiltinType::I128.into(),
                BuiltinValue::U128(_) => BuiltinType::U128.into(),
                BuiltinValue::F32(_) => BuiltinType::F32.into(),
                BuiltinValue::F64(_) => BuiltinType::F64.into(),
                BuiltinValue::String(_) => BuiltinType::String.into(),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::satn::Satn;
    use crate::{
        AlgebraicType, AlgebraicValue, ArrayType, BuiltinType, BuiltinValue, MapType, ProductType, ProductTypeElement,
        ProductValue, TypeInSpace, Typespace, ValueWithType,
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
        let sum_value = AlgebraicValue::OptionNone();
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
        let array = AlgebraicType::Builtin(BuiltinType::Array(ArrayType {
            elem_ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        }));
        let value = AlgebraicValue::Builtin(BuiltinValue::Array {
            val: Default::default(),
        });
        let typespace = Typespace::new(vec![]);
        assert_eq!(in_space(&typespace, &array, &value).to_satn(), "[]");
    }

    #[test]
    fn array_of_values() {
        let array = AlgebraicType::Builtin(BuiltinType::Array(ArrayType {
            elem_ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        }));
        let value = AlgebraicValue::Builtin(BuiltinValue::Array { val: vec![3u8].into() });
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
