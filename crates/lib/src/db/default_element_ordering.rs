//! This module defines the default ordering for fields of a `ProductType` and variants of a `SumType`.
//!
//! - In ABI version 8, the default ordering was not applied.
//! - In ABI version 9, the default ordering is applied to all types in a spacetime module, unless they explicitly declare a custom ordering.

use crate::is_sorted;
use spacetimedb_sats::{ProductType, ProductTypeElement, SumType, SumTypeVariant};

/// A label for a field of a `ProductType` or a variant of a `SumType`.
///
/// The ordering on this type defines the default ordering for the fields of a `ProductType` and the variants of a `SumType`.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum ElementLabel<'a> {
    /// An unnamed field with a position.
    /// The unnamed fields in a type do not necessarily have contiguous positions.
    Unnamed(usize),
    /// A named field.
    /// Names are required to be unique within the product type.
    Named(&'a str),
}

impl<'a> From<(usize, &'a ProductTypeElement)> for ElementLabel<'a> {
    fn from((i, element): (usize, &'a ProductTypeElement)) -> Self {
        match &element.name {
            Some(name) => ElementLabel::Named(&name[..]),
            None => ElementLabel::Unnamed(i),
        }
    }
}
impl<'a> From<(usize, &'a SumTypeVariant)> for ElementLabel<'a> {
    fn from((i, element): (usize, &'a SumTypeVariant)) -> Self {
        match &element.name {
            Some(name) => ElementLabel::Named(&name[..]),
            None => ElementLabel::Unnamed(i),
        }
    }
}

/// Checks if a sum type has the default ordering.
///
/// Not a recursive check.
pub fn sum_type_has_default_ordering(ty: &SumType) -> bool {
    is_sorted(ty.variants.iter().enumerate().map(ElementLabel::from))
}

/// Checks if a product type has the default ordering.
///
/// Not a recursive check.
pub fn product_type_has_default_ordering(ty: &ProductType) -> bool {
    is_sorted(ty.elements.iter().enumerate().map(ElementLabel::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::proptest;

    #[test]
    fn test_element_label_comparison() {
        let a = ElementLabel::Unnamed(0);
        let b = ElementLabel::Unnamed(2);
        let c = ElementLabel::Named("apples");
        let d = ElementLabel::Named("oranges");
        let e = ElementLabel::Named("oranges_tomorrow");

        assert!(a == a);
        assert!(a < b);
        assert!(a < c);
        assert!(a < d);
        assert!(a < e);

        assert!(b > a);
        assert!(b == b);
        assert!(b < c);
        assert!(b < d);
        assert!(b < e);

        assert!(c > a);
        assert!(c > b);
        assert!(c == c);
        assert!(c < d);
        assert!(c < e);

        assert!(d > a);
        assert!(d > b);
        assert!(d > c);
        assert!(d == d);
        assert!(d < e);

        assert!(e > a);
        assert!(e > b);
        assert!(e > c);
        assert!(e > d);
        assert!(e == e);
    }

    proptest! {
        #[test]
        fn test_is_sorted(v in proptest::collection::vec(0..100, 0..100)) {
            let mut v: Vec<i32> = v;
            v.sort();
            assert!(is_sorted(v.iter()));
        }
    }

    #[test]
    fn test_is_not_sorted() {
        assert!(!is_sorted([1, 2, 4, 3].iter()));
    }
}
