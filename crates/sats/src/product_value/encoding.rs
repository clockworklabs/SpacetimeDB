use super::ProductValue;
use crate::{algebraic_value::AlgebraicValue, product_type::ProductType};

impl ProductValue {
    pub fn decode(ty: &ProductType, bytes: impl AsRef<[u8]>) -> Result<(Self, usize), &'static str> {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        let len = ty.elements.len();

        let mut elements = Vec::new();
        for i in 0..len {
            let type_def = &ty.elements[i].algebraic_type;
            let (type_value, nr) = AlgebraicValue::decode(&type_def, &bytes[num_read..])?;
            num_read += nr;
            elements.push(type_value);
        }

        let tuple_value = ProductValue { elements };
        Ok((tuple_value, num_read))
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        for element in &self.elements {
            element.encode(bytes);
        }
    }
}
