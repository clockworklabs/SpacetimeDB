use crate::algebraic_type::AlgebraicType;
use crate::de::Deserialize;
use crate::meta_type::MetaType;
use crate::{impl_deserialize, impl_serialize};

/// An array type is a homegeneous product type of dynamic length.
///
/// That is, it is a product type
/// where every element / factor / field is of the same type
/// and where the length is statically unknown.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ArrayType {
    /// The base type every element of the array has.
    pub elem_ty: Box<AlgebraicType>,
}

impl_serialize!([] ArrayType, (self, ser) => self.elem_ty.serialize(ser));
impl_deserialize!([] ArrayType, de => Deserialize::deserialize(de).map(|elem_ty| Self { elem_ty }));

impl MetaType for ArrayType {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::ZERO_REF
    }
}
