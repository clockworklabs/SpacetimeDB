pub mod encoding;
pub mod satn;

use crate::algebraic_value::AlgebraicValue;

#[derive(Debug, Clone, Ord, PartialOrd, Hash)]
pub struct ProductValue {
    pub elements: Vec<AlgebraicValue>,
}

impl PartialEq for ProductValue {
    fn eq(&self, other: &Self) -> bool {
        if self.elements.len() != other.elements.len() {
            return false;
        }

        for i in 0..self.elements.len() {
            let x = &self.elements[i];
            let y = &other.elements[i];
            if x != y {
                return false;
            }
        }
        return true;
    }
}

impl Eq for ProductValue {}
