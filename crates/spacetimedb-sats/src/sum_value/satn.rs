use crate::satn::EntryWrapper;
use crate::{SumValue, ValueWithType};
use std::fmt;

impl<'a> crate::satn::Satn for ValueWithType<'a, SumValue> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ty().variants.is_empty() {
            panic!("This should not be possible.");
        }
        write!(f, "(")?;
        EntryWrapper::<','>::new(f).entry(|f| {
            let (ty, val) = (self.ty(), self.value());
            let e = &ty.variants[val.tag as usize];
            // if let Some(name) = &e.name {
            //     write!(f, "{}", name)?;
            //     write!(f, ": ")?;
            // }
            // write!(f, "{}", algebraic_type::SATNFormatter::new(&e.algebraic_type))?;

            if let Some(name) = &e.name {
                write!(f, "{}", name)?;
            }
            write!(f, " = ")?;
            self.with(&e.algebraic_type, &*val.value).fmt(f)
        })?;

        // if i < self.ty.variants.len() - 1 {
        //     write!(f, " | ")?;
        // }
        write!(f, ")")
    }
}
