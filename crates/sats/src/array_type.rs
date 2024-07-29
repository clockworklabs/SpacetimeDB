use crate::algebraic_type::AlgebraicType;
use crate::meta_type::MetaType;
use crate::SpacetimeType;

/// An array type is a homegeneous product type of dynamic length.
///
/// That is, it is a product type
/// where every element / factor / field is of the same type
/// and where the length is statically unknown.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, SpacetimeType)]
#[sats(crate = crate)]
pub struct ArrayType {
    /// The base type every element of the array has.
    pub elem_ty: Box<AlgebraicType>,
}

impl MetaType for ArrayType {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::ZERO_REF
    }
}
