use std::fmt;

use crate::{AlgebraicType, AlgebraicValue, ValueWithType};

impl<'a> crate::satn::Satn for ValueWithType<'a, AlgebraicValue> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ty() {
            AlgebraicType::Sum(ty) => {
                let val = self.value().as_sum().unwrap();
                self.with(ty, val).fmt(f)
            }
            AlgebraicType::Product(ty) => {
                let val = self.value().as_product().unwrap();
                self.with(ty, val).fmt(f)
            }
            AlgebraicType::Builtin(ty) => {
                let val = self.value().as_builtin().unwrap();
                self.with(ty, val).fmt(f)
            }
            AlgebraicType::Ref(r) => {
                let ty = &self.typespace()[*r];
                self.with(ty, self.value()).fmt(f)
            }
        }
    }
}
