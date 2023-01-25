use super::SumType;
use crate::algebraic_type;
use std::fmt::Display;

pub struct Formatter<'a> {
    ty: &'a SumType,
}

impl<'a> Formatter<'a> {
    pub fn new(ty: &'a SumType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for Formatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ty.variants.len() == 0 {
            return write!(f, "(|)");
        }
        write!(f, "(")?;
        for (i, e) in self.ty.variants.iter().enumerate() {
            if let Some(name) = &e.name {
                write!(f, "{}", name)?;
                write!(f, ": ")?;
            }
            write!(f, "{}", algebraic_type::satn::Formatter::new(&e.algebraic_type))?;
            if i < self.ty.variants.len() - 1 {
                write!(f, " | ")?;
            }
        }
        write!(f, ")")
    }
}
