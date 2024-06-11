use std::cmp::Ordering;

use crate::{AlgebraicType, BuiltinType, Typespace};

use super::def_new::Identifier;

/// Canonical
pub fn canonical_ordering(
    typespace: &Typespace,
    column_1: (&Identifier, &AlgebraicType),
    column_2: (&Identifier, &AlgebraicType),
) -> Ordering {
    alignment(column_1.1, typespace)
        .cmp(&alignment(column_2.1, typespace))
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
        AlgebraicType::Builtin(BuiltinType::Array(ref array)) => {
            let element_alignment = alignment(&array.elem_ty, ctx);
            element_alignment
        }
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
