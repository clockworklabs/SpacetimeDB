use std::ops::{Index, IndexMut};

use crate::algebraic_type::AlgebraicType;
use crate::algebraic_type_ref::AlgebraicTypeRef;
use crate::TypeInSpace;
use crate::{de::Deserialize, ser::Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[sats(crate = "crate")]
pub struct Typespace {
    pub root: AlgebraicTypeRef,
    pub types: Vec<AlgebraicType>,
}

impl Default for Typespace {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Index<AlgebraicTypeRef> for Typespace {
    type Output = AlgebraicType;

    fn index(&self, index: AlgebraicTypeRef) -> &Self::Output {
        &self.types[index.0 as usize]
    }
}
impl IndexMut<AlgebraicTypeRef> for Typespace {
    fn index_mut(&mut self, index: AlgebraicTypeRef) -> &mut Self::Output {
        &mut self.types[index.0 as usize]
    }
}

impl Typespace {
    pub const fn new(types: Vec<AlgebraicType>) -> Self {
        Self {
            root: AlgebraicTypeRef(0),
            types,
        }
    }

    pub fn new_with_root(root: AlgebraicTypeRef, types: Vec<AlgebraicType>) -> Self {
        Self { root, types }
    }

    pub fn root(&self) -> &AlgebraicType {
        &self[self.root]
    }

    pub fn get(&self, r: AlgebraicTypeRef) -> Option<&AlgebraicType> {
        self.types.get(r.0 as usize)
    }

    pub fn add(&mut self, ty: AlgebraicType) -> AlgebraicTypeRef {
        let i = self.types.len();
        self.types.push(ty);
        AlgebraicTypeRef(i as u32)
    }

    pub fn with_type<'a, T: ?Sized>(&'a self, ty: &'a T) -> TypeInSpace<'a, T> {
        TypeInSpace::new(self, ty)
    }
}
