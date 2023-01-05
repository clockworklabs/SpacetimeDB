use crate::{
    algebraic_value::{self, AlgebraicValue},
    sum_type::SumType,
};
use std::fmt::Display;
// use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct SumValue {
    pub tag: u8,
    pub value: Box<AlgebraicValue>,
}

pub struct SATNFormatter<'a> {
    ty: &'a SumType,
    val: &'a SumValue,
}

impl<'a> SATNFormatter<'a> {
    pub fn new(ty: &'a SumType, val: &'a SumValue) -> Self {
        Self { ty, val }
    }
}

impl<'a> Display for SATNFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ty = self.ty.types.get(self.val.tag as usize).unwrap();
        write!(f, "{}", algebraic_value::SATNFormatter::new(ty, &self.val.value))
    }
}

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
        let type_def = loop {
            let item = &ty.types[i];
            if i as u8 == tag {
                break item;
            }
            i += 1;
        };
        let (type_value, nr) = AlgebraicValue::decode(&type_def, &bytes[num_read..])?;
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
