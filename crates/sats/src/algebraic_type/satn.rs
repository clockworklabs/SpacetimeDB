use super::AlgebraicType;
use crate::{builtin_type, product_type, sum_type};
use std::fmt::Display;

/// NOTE: You might ask: Why do we have a formatter and a notation for
/// `AlgebraicType`s if we don't have an encoding for `AlgebraicType`s?
///
/// This is because we just want an easier to read text format for algebraic
/// types. This could just as easily take in an algebraic value, which
/// represents an algebraic type and format it that way. It's just more
/// convenient to format it from the Rust type.
pub struct Formatter<'a> {
    ty: &'a AlgebraicType,
}

impl<'a> Formatter<'a> {
    pub fn new(ty: &'a AlgebraicType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for Formatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            AlgebraicType::Sum(ty) => {
                write!(f, "{}", sum_type::satn::Formatter::new(ty))
            }
            AlgebraicType::Product(ty) => {
                write!(f, "{}", product_type::satn::Formatter::new(ty))
            }
            AlgebraicType::Builtin(p) => {
                write!(f, "{}", builtin_type::satn::Formatter::new(p))
            }
            AlgebraicType::Ref(r) => {
                write!(f, "{}", r)
            }
        }
    }
}
