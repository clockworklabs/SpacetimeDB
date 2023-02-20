use super::SumValue;
use crate::{algebraic_value::AlgebraicValue, sum_type::SumType};

impl SumValue {
    pub fn decode(ty: &SumType, bytes: impl AsRef<[u8]>) -> Result<(Self, usize), &'static str> {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return Err("Byte array length is invalid.");
        }
        let tag = bytes[num_read];
        num_read += 1;

        let mut i = 0;
        let variant = loop {
            let item = &ty.variants[i];
            if i as u8 == tag {
                break item;
            }
            i += 1;
        };
        let (type_value, nr) = AlgebraicValue::decode(&variant.algebraic_type, &bytes[num_read..])?;
        num_read += nr;

        Ok((
            SumValue {
                tag,
                value: Box::new(type_value),
            },
            num_read,
        ))
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.tag);
        self.value.encode(bytes);
    }
}
