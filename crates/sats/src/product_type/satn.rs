use super::ProductType;
use crate::algebraic_type;
use std::fmt::Display;

pub struct Formatter<'a> {
    ty: &'a ProductType,
}

impl<'a> Formatter<'a> {
    pub fn new(ty: &'a ProductType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for Formatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        for (i, e) in self.ty.elements.iter().enumerate() {
            if let Some(name) = &e.name {
                write!(f, "{}", name)?;
            } else {
                write!(f, "{}", i)?;
            }
            write!(f, ": ")?;
            write!(f, "{}", algebraic_type::satn::Formatter::new(&e.algebraic_type))?;
            if i < self.ty.elements.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")
    }
}
