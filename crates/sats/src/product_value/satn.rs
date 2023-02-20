use crate::satn::EntryWrapper;
use crate::{ProductValue, ValueWithType};
use std::fmt;

impl<'a> crate::satn::Satn for ValueWithType<'a, ProductValue> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        let (ty, val) = (self.ty(), self.value());
        EntryWrapper::<','>::new(f).entries(ty.elements.iter().zip(&val.elements).enumerate().map(
            |(i, (e_ty, e_val))| {
                move |f: &mut fmt::Formatter| {
                    if let Some(name) = &e_ty.name {
                        write!(f, "{}", name)?;
                    } else {
                        write!(f, "{}", i)?;
                    }
                    // write!(f, ": ")?;
                    // write!(f, "{}", algebraic_type::SATNFormatter::new(&e_ty.algebraic_type))?;
                    write!(f, " = ")?;
                    self.with(&e_ty.algebraic_type, e_val).fmt(f)
                }
            },
        ))?;
        write!(f, ")")
    }
}
