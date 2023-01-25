use super::SumValue;
use crate::{algebraic_value, sum_type::SumType, typespace::Typespace};
use std::fmt::Display;

pub struct Formatter<'a> {
    typespace: &'a Typespace,
    ty: &'a SumType,
    val: &'a SumValue,
}

impl<'a> Formatter<'a> {
    pub fn new(typespace: &'a Typespace, ty: &'a SumType, val: &'a SumValue) -> Self {
        Self { typespace, ty, val }
    }
}

impl<'a> Display for Formatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ty.variants.len() == 0 {
            panic!("This should not be possible.");
        }
        write!(f, "(")?;
        for (i, e) in self.ty.variants.iter().enumerate() {
            // if let Some(name) = &e.name {
            //     write!(f, "{}", name)?;
            //     write!(f, ": ")?;
            // }
            // write!(f, "{}", algebraic_type::SATNFormatter::new(&e.algebraic_type))?;

            if i == self.val.tag as usize {
                if let Some(name) = &e.name {
                    write!(f, "{}", name)?;
                }
                write!(f, " = ")?;
                let e_ty = &self.ty.variants[i];
                write!(
                    f,
                    "{}",
                    algebraic_value::satn::Formatter::new(self.typespace, &e_ty.algebraic_type, &self.val.value)
                )?;
            }

            // if i < self.ty.variants.len() - 1 {
            //     write!(f, " | ")?;
            // }
        }
        write!(f, ")")
    }
}
