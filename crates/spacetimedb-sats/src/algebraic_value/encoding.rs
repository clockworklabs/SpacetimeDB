use super::AlgebraicValue;
use crate::{
    algebraic_type::AlgebraicType, builtin_value::BuiltinValue, product_value::ProductValue, sum_value::SumValue,
};

impl AlgebraicValue {
    pub fn decode(algebraic_type: &AlgebraicType, bytes: impl AsRef<[u8]>) -> Result<(Self, usize), &'static str> {
        let bytes = bytes.as_ref();
        match algebraic_type {
            AlgebraicType::Product(ty) => {
                let (val, nr) = ProductValue::decode(ty, &bytes[0..])?;
                Ok((Self::Product(val), nr))
            }
            AlgebraicType::Sum(ty) => {
                let (val, nr) = SumValue::decode(ty, &bytes[0..])?;
                Ok((Self::Sum(val), nr))
            }
            AlgebraicType::Builtin(ty) => {
                let (val, nr) = BuiltinValue::decode(ty, &bytes[0..])?;
                Ok((Self::Builtin(val), nr))
            }
            AlgebraicType::Ref(_) => todo!(),
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            Self::Product(v) => {
                v.encode(bytes);
            }
            Self::Sum(v) => {
                v.encode(bytes);
            }
            Self::Builtin(v) => {
                v.encode(bytes);
            }
        }
    }
}
