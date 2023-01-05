use crate::algebraic_type::{self, AlgebraicType};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

// TODO: probably implement this with a tuple but store whether the tuple
// is a sum tuple or a product tuple, then we have uniformity over types
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SumType {
    pub types: Vec<AlgebraicType>,
}

pub struct SATNFormatter<'a> {
    ty: &'a SumType,
}

impl<'a> SATNFormatter<'a> {
    pub fn new(ty: &'a SumType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for SATNFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ty.types.len() == 0 {
            return write!(f, "|");
        }
        write!(f, "(")?;
        for (i, e) in self.ty.types.iter().enumerate() {
            write!(f, "{}", algebraic_type::SATNFormatter::new(e))?;
            if i < self.ty.types.len() - 1 {
                write!(f, " | ")?;
            }
        }
        write!(f, ")")
    }
}

impl SumType {
    pub fn new(types: Vec<AlgebraicType>) -> Self {
        Self { types }
    }
}

impl SumType {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() <= 0 {
            return Err("Bytes array length is invalid.".to_string());
        }

        let len = bytes[num_read];
        num_read += 1;

        let mut items = Vec::new();
        for _ in 0..len {
            let (item, nr) = AlgebraicType::decode(&bytes[num_read..])?;
            items.push(item);
            num_read += nr;
        }
        Ok((SumType { types: items }, num_read))
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.types.len() as u8);
        for item in &self.types {
            item.encode(bytes);
        }
    }
}
