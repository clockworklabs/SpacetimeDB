use std::cmp::Ordering;
use std::fmt::Debug;

use spacetimedb_sats::{AlgebraicType, BuiltinType, Typespace};

/// Canonical ordering for fields of a Table.
/// Does not currently apply to all AlgebraicTypes.
pub fn canonical_column_ordering(
    typespace: &Typespace,
    column_1: (&str, &AlgebraicType),
    column_2: (&str, &AlgebraicType),
) -> Ordering {
    alignment(column_1.1, typespace)
        .cmp(&alignment(column_2.1, typespace))
        .reverse() // big goes before small
        .then_with(|| column_1.0.cmp(column_2.0))
}

/// Determine the alignment of a value of this algebraic type.
pub fn alignment(type_: &AlgebraicType, ctx: &Typespace) -> u8 {
    // TODO(jgilles): I'm not sure about all these values. This is a first approximation.
    // These types may in fact have different alignments in different contexts, as seen in the `table` crate.
    // I don't want to make that a dependency of `sats` though.
    match type_ {
        AlgebraicType::Builtin(BuiltinType::Bool) => 1,
        AlgebraicType::Builtin(BuiltinType::I8) => 1,
        AlgebraicType::Builtin(BuiltinType::I16) => 2,
        AlgebraicType::Builtin(BuiltinType::I32) => 4,
        AlgebraicType::Builtin(BuiltinType::I64) => 8,
        AlgebraicType::Builtin(BuiltinType::I128) => 16,
        AlgebraicType::Builtin(BuiltinType::U8) => 1,
        AlgebraicType::Builtin(BuiltinType::U16) => 2,
        AlgebraicType::Builtin(BuiltinType::U32) => 4,
        AlgebraicType::Builtin(BuiltinType::U64) => 8,
        AlgebraicType::Builtin(BuiltinType::U128) => 16,
        AlgebraicType::Builtin(BuiltinType::F32) => 4,
        AlgebraicType::Builtin(BuiltinType::F64) => 8,
        AlgebraicType::Builtin(BuiltinType::String) => 8,
        AlgebraicType::Builtin(BuiltinType::Array(ref array)) => alignment(&array.elem_ty, ctx),
        AlgebraicType::Builtin(BuiltinType::Map(ref _map)) => 8,
        // Minimum possible alignment is 1, even though minimum possible size is 0.
        // This is consistent with Rust.
        AlgebraicType::Product(ref product) => product
            .elements
            .iter()
            .map(|child| alignment(&child.algebraic_type, ctx))
            .max()
            .unwrap_or(1),
        AlgebraicType::Sum(ref sum) => sum
            .variants
            .iter()
            .map(|variant| alignment(&variant.algebraic_type, ctx))
            .max()
            .unwrap_or(1),
        AlgebraicType::Ref(ref ref_) => alignment(&ctx[*ref_], ctx),
    }
}

/// Check that a slice is sorted by an ordering.
pub fn is_sorted_by<T: Debug>(v: &[T], cmp: impl Fn(&T, &T) -> Ordering) -> bool {
    v.windows(2).all(|w| cmp(&w[0], &w[1]) == Ordering::Less)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlgebraicType, ProductType, ProductTypeElement};

    #[test]
    fn test_comparison() {
        let typespace = Typespace::default();
        let a = ("a", &AlgebraicType::Builtin(BuiltinType::I64));
        let b = ("b", &AlgebraicType::Builtin(BuiltinType::I32));
        let c = ("c", &AlgebraicType::Builtin(BuiltinType::I8));
        let d = ("d", &AlgebraicType::Builtin(BuiltinType::I8));

        assert_eq!(canonical_column_ordering(&typespace, a, a), Ordering::Equal);

        assert_eq!(canonical_column_ordering(&typespace, a, b), Ordering::Less);
        assert_eq!(canonical_column_ordering(&typespace, b, a), Ordering::Greater);

        assert_eq!(canonical_column_ordering(&typespace, b, b), Ordering::Equal);

        assert_eq!(canonical_column_ordering(&typespace, b, c), Ordering::Less);
        assert_eq!(canonical_column_ordering(&typespace, c, b), Ordering::Greater);

        assert_eq!(canonical_column_ordering(&typespace, c, c), Ordering::Equal);

        assert_eq!(canonical_column_ordering(&typespace, c, d), Ordering::Less);
        assert_eq!(canonical_column_ordering(&typespace, d, c), Ordering::Greater);

        assert_eq!(canonical_column_ordering(&typespace, d, d), Ordering::Equal);
    }

    #[test]
    fn test_in_typespace() {
        let product_type = AlgebraicType::Product(ProductType {
            elements: vec![
                ProductTypeElement {
                    name: Some("a".into()),
                    algebraic_type: AlgebraicType::U64,
                },
                ProductTypeElement {
                    name: Some("b".into()),
                    algebraic_type: AlgebraicType::String,
                },
            ]
            .into_boxed_slice(),
        });
        let mut typespace = Typespace::default();
        let product_type_ref = typespace.add(product_type.clone());

        assert_eq!(
            alignment(&product_type, &typespace),
            alignment(&AlgebraicType::String, &typespace)
        );
        assert_eq!(
            alignment(&AlgebraicType::Ref(product_type_ref), &typespace),
            alignment(&product_type, &typespace)
        );
    }

    #[test]
    fn test_is_sorted_by() {
        assert!(is_sorted_by(&[1, 2, 3, 4], |a, b| a.cmp(b)));
        assert!(!is_sorted_by(&[1, 2, 4, 3], |a, b| a.cmp(b)));
    }
}
