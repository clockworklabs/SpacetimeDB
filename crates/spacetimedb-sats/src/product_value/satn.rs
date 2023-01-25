use super::ProductValue;
use crate::{algebraic_value, product_type::ProductType, typespace::Typespace};
use std::fmt::Display;

pub struct Formatter<'a> {
    typespace: &'a Typespace,
    ty: &'a ProductType,
    val: &'a ProductValue,
}

impl<'a> Formatter<'a> {
    pub fn new(typespace: &'a Typespace, ty: &'a ProductType, val: &'a ProductValue) -> Self {
        Self { typespace, ty, val }
    }
}

impl<'a> Display for Formatter<'a> {
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
            // write!(f, ": ")?;
            // write!(f, "{}", algebraic_type::SATNFormatter::new(&e_ty.algebraic_type))?;
            write!(
                f,
                " = {}",
                algebraic_value::satn::Formatter::new(self.typespace, &e_ty.algebraic_type, e_val)
            )?;
            if i < self.ty.elements.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")
    }
}
