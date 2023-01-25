use crate::algebraic_type::AlgebraicType;

pub struct Typespace {
    pub root: usize,
    pub types: Vec<AlgebraicType>,
}

impl Typespace {
    pub fn new(types: Vec<AlgebraicType>) -> Self {
        Self { root: 0, types }
    }

    pub fn new_with_root(root: usize, types: Vec<AlgebraicType>) -> Self {
        Self { root, types }
    }
}
