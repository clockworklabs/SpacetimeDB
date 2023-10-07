use std::cmp::Ordering;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use crate::algebraic_value::AlgebraicValue;
use crate::static_assert_size;
use crate::sum_type::SumType;

/// A value of a sum type chosing a specific variant of the type.
#[repr(packed)] // Shrink size enough to allow `size_of(AlgebraicValue) = 16`.
#[derive(Eq)]
pub struct SumValue {
    /// Given a variant `Var(Ty)` in a sum type `{ Var(Ty), ... }`,
    /// this provides the `value` for `Ty`.
    pub value: Box<AlgebraicValue>,
    /// A tag representing the choice of one variant of the sum type's variants.
    pub tag: u8,
}

#[cfg(target_arch = "wasm32")]
static_assert_size!(SumValue, 5);
#[cfg(not(target_arch = "wasm32"))]
static_assert_size!(SumValue, 9);

impl SumValue {
    /// Returns the tag and and a reference to the value.
    pub fn parts(&self) -> (u8, &AlgebraicValue) {
        (self.tag, &*self.value)
    }
}

impl Debug for SumValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (t, v) = self.parts();
        f.debug_struct("SumValue").field("tag", &t).field("value", v).finish()
    }
}

impl Clone for SumValue {
    fn clone(&self) -> Self {
        let (tag, value) = self.parts();
        let value = Box::new(value.clone());
        Self { tag, value }
    }
}

impl PartialEq for SumValue {
    fn eq(&self, other: &Self) -> bool {
        self.parts() == other.parts()
    }
}

impl Hash for SumValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.parts().hash(state);
    }
}

impl Ord for SumValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts().cmp(&other.parts())
    }
}

impl PartialOrd for SumValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.parts().partial_cmp(&other.parts())
    }
}

impl crate::Value for SumValue {
    type Type = SumType;
}
