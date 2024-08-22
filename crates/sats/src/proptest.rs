//! Proptest generators for the subset of `AlgebraicType` and `AlgebraicValue` which our tables can store.
//!
//! This notably excludes `Ref` types.

use crate::{i256, u256};
use crate::{
    AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ArrayValue, MapType, MapValue, ProductType, ProductValue, SumType,
    SumValue, Typespace, F32, F64,
};
use proptest::{
    collection::{vec, SizeRange},
    prelude::*,
    prop_oneof,
};

const SIZE: usize = 16;

/// Generates leaf (i.e. non-compound) `AlgebraicType`s.
///
/// These are types which do not contain other types,
/// i.e. bools, integers, floats, strings and units.
///
/// All other generated `AlgebraicType`s  are compound,
/// i.e. contain at least one child `AlgebraicType`,
/// and thus require recursive generation.
fn generate_non_compound_algebraic_type() -> impl Strategy<Value = AlgebraicType> {
    prop_oneof![
        Just(AlgebraicType::Bool),
        Just(AlgebraicType::U8),
        Just(AlgebraicType::I8),
        Just(AlgebraicType::U16),
        Just(AlgebraicType::I16),
        Just(AlgebraicType::U32),
        Just(AlgebraicType::I32),
        Just(AlgebraicType::U64),
        Just(AlgebraicType::I64),
        Just(AlgebraicType::U128),
        Just(AlgebraicType::I128),
        Just(AlgebraicType::U256),
        Just(AlgebraicType::I256),
        Just(AlgebraicType::F32),
        Just(AlgebraicType::F64),
        Just(AlgebraicType::String),
        Just(AlgebraicType::unit()),
    ]
}

/// Generate an algebraic type wrapping leaf types.
fn generate_algebraic_type_from_leaves(
    leaves: impl Strategy<Value = AlgebraicType> + 'static,
    depth: u32,
) -> impl Strategy<Value = AlgebraicType> {
    leaves.prop_recursive(depth, SIZE as u32, SIZE as u32, |gen_element| {
        prop_oneof![
            gen_element.clone().prop_map(AlgebraicType::array),
            (gen_element.clone(), gen_element.clone()).prop_map(|(key, val)| AlgebraicType::map(key, val)),
            // No need for field or variant names.

            // No need to generate units here;
            // we already generate them in `generate_non_compound_algebraic_type`.
            vec(gen_element.clone().prop_map_into(), 1..=SIZE)
                .prop_map(Vec::into_boxed_slice)
                .prop_map(AlgebraicType::product),
            // Do not generate nevers here; we can't store never in a page.
            vec(gen_element.clone().prop_map_into(), 1..=SIZE)
                .prop_map(Vec::into_boxed_slice)
                .prop_map(AlgebraicType::sum),
        ]
    })
}

/// Generates `AlgebraicType`s, not including recursive (i.e. `Ref` types),
/// but including compound types (i.e. `Product` and `Sum` types).
///
/// Any type generated here is valid as a column in a row type.
pub fn generate_algebraic_type() -> impl Strategy<Value = AlgebraicType> {
    generate_algebraic_type_from_leaves(generate_non_compound_algebraic_type(), 4)
}

/// Generates a `ProductType` that is good as a row type.
pub fn generate_row_type(range: impl Into<SizeRange>) -> impl Strategy<Value = ProductType> {
    vec(generate_algebraic_type().prop_map_into(), range)
        .prop_map(Vec::into_boxed_slice)
        .prop_map_into()
}

/// Generates an `AlgebraicValue` for values `Val: Arbitrary`.
fn generate_non_compound<Val: Arbitrary + Into<AlgebraicValue> + 'static>() -> BoxedStrategy<AlgebraicValue> {
    any::<Val>().prop_map(Into::into).boxed()
}

fn any_u256() -> impl Strategy<Value = u256> {
    any::<(u128, u128)>().prop_map(|(hi, lo)| u256::from_words(hi, lo))
}

fn any_i256() -> impl Strategy<Value = i256> {
    any::<(i128, i128)>().prop_map(|(hi, lo)| i256::from_words(hi, lo))
}

/// Generates an `AlgebraicValue` typed at `ty`.
pub fn generate_algebraic_value(ty: AlgebraicType) -> impl Strategy<Value = AlgebraicValue> {
    match ty {
        AlgebraicType::Bool => generate_non_compound::<bool>(),
        AlgebraicType::I8 => generate_non_compound::<i8>(),
        AlgebraicType::U8 => generate_non_compound::<u8>(),
        AlgebraicType::I16 => generate_non_compound::<i16>(),
        AlgebraicType::U16 => generate_non_compound::<u16>(),
        AlgebraicType::I32 => generate_non_compound::<i32>(),
        AlgebraicType::U32 => generate_non_compound::<u32>(),
        AlgebraicType::I64 => generate_non_compound::<i64>(),
        AlgebraicType::U64 => generate_non_compound::<u64>(),
        AlgebraicType::I128 => generate_non_compound::<i128>(),
        AlgebraicType::U128 => generate_non_compound::<u128>(),
        AlgebraicType::I256 => any_i256().prop_map_into().boxed(),
        AlgebraicType::U256 => any_u256().prop_map_into().boxed(),
        AlgebraicType::F32 => generate_non_compound::<f32>(),
        AlgebraicType::F64 => generate_non_compound::<f64>(),
        AlgebraicType::String => generate_non_compound::<Box<str>>(),

        AlgebraicType::Array(ty) => generate_array_value(*ty.elem_ty).prop_map_into().boxed(),

        AlgebraicType::Map(ty) => generate_map_value(*ty).prop_map_into().boxed(),

        AlgebraicType::Product(ty) => generate_product_value(ty).prop_map_into().boxed(),

        AlgebraicType::Sum(ty) => generate_sum_value(ty).prop_map_into().boxed(),

        AlgebraicType::Ref(_) => unreachable!(),
    }
}

/// Generates a `ProductValue` typed at `ty`.
pub fn generate_product_value(ty: ProductType) -> impl Strategy<Value = ProductValue> {
    Vec::from(ty.elements)
        .into_iter()
        .map(|elem| generate_algebraic_value(elem.algebraic_type))
        .collect::<Vec<_>>()
        .prop_map_into()
}

/// Generates a `SumValue` typed at `ty`.
fn generate_sum_value(ty: SumType) -> impl Strategy<Value = SumValue> {
    // A dependent problem, generate a tag
    // and then generate a value typed at the tag' data type.
    (0..ty.variants.len()).prop_flat_map(move |tag: usize| {
        let variant_ty = ty.variants[tag].clone();
        let gen_variant = generate_algebraic_value(variant_ty.algebraic_type);
        gen_variant.prop_map(move |value| SumValue {
            tag: tag as u8,
            value: Box::new(value),
        })
    })
}

/// Generates a `MapValue` typed at `ty`.
fn generate_map_value(ty: MapType) -> impl Strategy<Value = MapValue> {
    vec(
        (generate_algebraic_value(ty.key_ty), generate_algebraic_value(ty.ty)),
        0..=SIZE,
    )
    .prop_map(|entries| entries.into_iter().collect())
}

/// Generates an array value given an element generator `gen_elem`.
fn generate_array_of<S>(gen_elem: S) -> BoxedStrategy<ArrayValue>
where
    S: Strategy + 'static,
    Box<[S::Value]>: 'static + Into<ArrayValue>,
{
    vec(gen_elem, 0..=SIZE)
        .prop_map(Vec::into_boxed_slice)
        .prop_map_into()
        .boxed()
}

/// Generates an array value with elements typed at `ty`.
fn generate_array_value(ty: AlgebraicType) -> BoxedStrategy<ArrayValue> {
    match ty {
        AlgebraicType::Bool => generate_array_of(any::<bool>()),
        AlgebraicType::I8 => generate_array_of(any::<i8>()),
        AlgebraicType::U8 => generate_array_of(any::<u8>()),
        AlgebraicType::I16 => generate_array_of(any::<i16>()),
        AlgebraicType::U16 => generate_array_of(any::<u16>()),
        AlgebraicType::I32 => generate_array_of(any::<i32>()),
        AlgebraicType::U32 => generate_array_of(any::<u32>()),
        AlgebraicType::I64 => generate_array_of(any::<i64>()),
        AlgebraicType::U64 => generate_array_of(any::<u64>()),
        AlgebraicType::I128 => generate_array_of(any::<i128>()),
        AlgebraicType::U128 => generate_array_of(any::<u128>()),
        AlgebraicType::I256 => generate_array_of(any_i256()),
        AlgebraicType::U256 => generate_array_of(any_u256()),
        AlgebraicType::F32 => generate_array_of(any::<f32>().prop_map_into::<F32>()),
        AlgebraicType::F64 => generate_array_of(any::<f64>().prop_map_into::<F64>()),
        AlgebraicType::String => generate_array_of(any::<Box<str>>()),
        AlgebraicType::Product(ty) => generate_array_of(generate_product_value(ty)),
        AlgebraicType::Sum(ty) => generate_array_of(generate_sum_value(ty)),
        AlgebraicType::Array(ty) => generate_array_of(generate_array_value(*ty.elem_ty)),
        AlgebraicType::Map(ty) => generate_array_of(generate_map_value(*ty)),
        AlgebraicType::Ref(_) => unreachable!(),
    }
}

/// Generates a row type `ty` and a row value typed at `ty`.
pub fn generate_typed_row() -> impl Strategy<Value = (ProductType, ProductValue)> {
    generate_row_type(0..=SIZE).prop_flat_map(|ty| (Just(ty.clone()), generate_product_value(ty)))
}

/// Generates a type `ty` and a value typed at `ty`.
pub fn generate_typed_value() -> impl Strategy<Value = (AlgebraicType, AlgebraicValue)> {
    generate_algebraic_type().prop_flat_map(|ty| (Just(ty.clone()), generate_algebraic_value(ty)))
}

/// Generate a `Ref` to something in a `Typespace` of this length.
fn generate_ref(typespace_len: u32) -> BoxedStrategy<AlgebraicType> {
    (0..typespace_len).prop_map(|n| AlgebraicTypeRef(n).into()).boxed()
}

/// Generate a type valid to be used to generate a type *use* in a client module.
/// That is, a ref, non-compound type, a special type, or an array, map, or option of the same.
fn generate_type_valid_for_client_use() -> impl Strategy<Value = AlgebraicType> {
    let leaf = prop_oneof![
        generate_non_compound_algebraic_type(),
        Just(AlgebraicType::identity()),
        Just(AlgebraicType::address()),
    ];

    let size = 3;

    leaf.prop_recursive(size, size, size, |gen_element| {
        prop_oneof![
            gen_element.clone().prop_map(AlgebraicType::array),
            (gen_element.clone(), gen_element.clone()).prop_map(|(key, val)| AlgebraicType::map(key, val)),
            gen_element.clone().prop_map(AlgebraicType::option),
        ]
    })
}

/// Generate a `Typespace` valid for client code generation with `size` elements.
///
/// We don't prop_map on the size because it supposedly can lead to exponential shrinking times.
///
/// Does not generate nested arrays or maps currently, although these would be allowed.
pub fn generate_typespace_valid_for_codegen(size: u32) -> impl Strategy<Value = Typespace> {
    let generate_value = generate_type_valid_for_client_use().boxed();

    let types = (0..size)
        .map(|current_len| {
            let leaf = if current_len == 0 {
                generate_value.clone()
            } else {
                generate_value.clone().prop_union(generate_ref(current_len)).boxed()
            };
            // depth 1 means these will either be leaves or a single level of nesting.
            generate_algebraic_type_from_leaves(leaf, 1)
        })
        .collect::<Vec<_>>();

    types.prop_map(FromIterator::from_iter)
}
