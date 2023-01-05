use crate::{algebraic_type, product_type_element::ProductTypeElement};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ProductType {
    pub elements: Vec<ProductTypeElement>,
}

impl ProductType {
    pub fn new(elements: Vec<ProductTypeElement>) -> Self {
        Self { elements }
    }
}

pub struct SATNFormatter<'a> {
    ty: &'a ProductType,
}

impl<'a> SATNFormatter<'a> {
    pub fn new(ty: &'a ProductType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for SATNFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        for (i, e) in self.ty.elements.iter().enumerate() {
            if let Some(name) = &e.name {
                write!(f, "{}", name)?;
            } else {
                write!(f, "{}", i)?;
            }
            write!(f, ": ")?;
            write!(f, "{}", algebraic_type::SATNFormatter::new(&e.algebraic_type))?;
            if i < self.ty.elements.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")
    }
}

impl ProductType {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return Err("TupleDef::decode: byte array has invalid length.".to_string());
        }

        let len = bytes[num_read];
        num_read += 1;

        let mut elements = Vec::new();
        for _ in 0..len {
            let (element, nr) = ProductTypeElement::decode(&bytes[num_read..])?;
            elements.push(element);
            num_read += nr;
        }
        Ok((ProductType { elements }, num_read))
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.elements.len() as u8);
        for item in &self.elements {
            item.encode(bytes);
        }
    }
}
