use crate::{
    algebraic_type,
    algebraic_value::{self, AlgebraicValue},
    product_type::ProductType,
};
use std::fmt::Display;
// use serde::{Deserialize, Serialize};
// use std::fmt::Display;
// use std::hash::Hash;

#[derive(Debug, Clone, Ord, PartialOrd)]
pub struct ProductValue {
    pub elements: Vec<AlgebraicValue>,
}

pub struct SATNFormatter<'a> {
    ty: &'a ProductType,
    val: &'a ProductValue,
}

impl<'a> SATNFormatter<'a> {
    pub fn new(ty: &'a ProductType, val: &'a ProductValue) -> Self {
        Self { ty, val }
    }
}

impl<'a> Display for SATNFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        for i in 0..self.ty.elements.len() {
            let e_ty = &self.ty.elements[i];
            let e_val = &self.val.elements[i];
            if let Some(name) = &e_ty.name {
                write!(f, "{}", name)?;
            } else {
                write!(f, "{}", i)?;
            }
            write!(f, ": ")?;
            write!(f, "{}", algebraic_type::SATNFormatter::new(&e_ty.algebraic_type))?;
            write!(
                f,
                " = {}",
                algebraic_value::SATNFormatter::new(&e_ty.algebraic_type, e_val)
            )?;
            if i < self.ty.elements.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")
    }
}

impl PartialEq for ProductValue {
    fn eq(&self, other: &Self) -> bool {
        if self.elements.len() != other.elements.len() {
            return false;
        }

        for i in 0..self.elements.len() {
            let x = &self.elements[i];
            let y = &other.elements[i];
            if x != y {
                return false;
            }
        }
        return true;
    }
}

impl Eq for ProductValue {}

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
