use super::AlgebraicValue;
use crate::{algebraic_type::AlgebraicType, builtin_value, product_value, sum_value, typespace::Typespace};
use std::fmt::Display;

pub struct Formatter<'a> {
    typespace: &'a Typespace,
    ty: &'a AlgebraicType,
    val: &'a AlgebraicValue,
}

impl<'a> Formatter<'a> {
    pub fn new(typespace: &'a Typespace, ty: &'a AlgebraicType, val: &'a AlgebraicValue) -> Self {
        Self { typespace, ty, val }
    }
}

impl<'a> Display for Formatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            AlgebraicType::Sum(ty) => {
                let val = self.val.as_sum().unwrap();
                write!(f, "{}", sum_value::satn::Formatter::new(self.typespace, ty, val))
            }
            AlgebraicType::Product(ty) => {
                let val = self.val.as_product().unwrap();
                write!(f, "{}", product_value::satn::Formatter::new(self.typespace, ty, val))
            }
            AlgebraicType::Builtin(ty) => {
                let val = self.val.as_builtin().unwrap();
                write!(f, "{}", builtin_value::satn::Formatter::new(self.typespace, ty, val))
            }
            AlgebraicType::Ref(r) => {
                let ty = &self.typespace.types[r.0 as usize];
                write!(f, "{}", self::Formatter::new(self.typespace, ty, self.val))
            }
        }
    }
}
