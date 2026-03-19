use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef};

pub const QUERY_VIEW_RETURN_TAG: &str = "__query__";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Procedural,
    Query,
}

pub fn extract_view_return_product_type_ref(return_type: &AlgebraicType) -> Option<(AlgebraicTypeRef, ViewKind)> {
    // Query-builder views (`Query<T>`) are encoded as: { __query__: T }.
    if let Some(product) = return_type.as_product()
        && product.elements.len() == 1
        && product.elements[0].name.as_deref() == Some(QUERY_VIEW_RETURN_TAG)
        && let Some(product_type_ref) = product.elements[0].algebraic_type.as_ref().copied()
    {
        return Some((product_type_ref, ViewKind::Query));
    }

    return_type
        .as_option()
        .and_then(AlgebraicType::as_ref)
        .or_else(|| {
            return_type
                .as_array()
                .map(|array_type| array_type.elem_ty.as_ref())
                .and_then(AlgebraicType::as_ref)
        })
        .copied()
        .map(|product_type_ref| (product_type_ref, ViewKind::Procedural))
}
