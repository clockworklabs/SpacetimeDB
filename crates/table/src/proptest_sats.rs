//! Proptest generators for the subset of `AlgebraicType` and `AlgebraicValue` which our tables can store.
//!
//! This notably excludes `Ref` types.

use proptest::{
    collection::{vec, SizeRange},
    prelude::*,
    prop_oneof,
    strategy::Just,
    strategy::{BoxedStrategy, Strategy},
};
use spacetimedb_sats::{
    AlgebraicType, AlgebraicValue, ArrayValue, BuiltinType, MapType, MapValue, ProductType, ProductValue, SumType,
    SumValue, F32, F64,
};

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
        Just(AlgebraicType::F32),
        Just(AlgebraicType::F64),
        Just(AlgebraicType::String),
        Just(AlgebraicType::unit()),
    ]
}

/// Generates `AlgebraicType`s, not including recursive (i.e. `Ref` types),
/// but including compound types (i.e. `Product` and `Sum` types).
///
/// Any type generated here is valid as a column in a row type.
pub fn generate_algebraic_type() -> impl Strategy<Value = AlgebraicType> {
    generate_non_compound_algebraic_type().prop_recursive(4, 16, 16, |gen_element| {
        prop_oneof![
            gen_element.clone().prop_map(AlgebraicType::array),
            (gen_element.clone(), gen_element.clone()).prop_map(|(key, val)| AlgebraicType::map(key, val)),
            // No need for field or variant names.

            // No need to generate units here;
            // we already generate them in `generate_non_compound_algebraic_type`.
            vec(gen_element.clone().prop_map_into(), 1..=16).prop_map(AlgebraicType::product),
            // Do not generate nevers here; we can't store never in a page.
            vec(gen_element.clone().prop_map_into(), 1..=16).prop_map(AlgebraicType::sum),
        ]
    })
}

/// Generates a `ProductType` that is good as a row type.
pub fn generate_row_type(range: impl Into<SizeRange>) -> impl Strategy<Value = ProductType> {
    vec(generate_algebraic_type().prop_map_into(), range).prop_map_into()
}

/// Generates an `AlgebraicValue` for values `Val: Arbitrary`.
fn generate_non_compound<Val: Arbitrary + Into<AlgebraicValue> + 'static>() -> BoxedStrategy<AlgebraicValue> {
    any::<Val>().prop_map(Into::into).boxed()
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
        AlgebraicType::F32 => generate_non_compound::<f32>(),
        AlgebraicType::F64 => generate_non_compound::<f64>(),
        AlgebraicType::String => generate_non_compound::<String>(),

        AlgebraicType::Builtin(BuiltinType::Array(ty)) => generate_array_value(*ty.elem_ty).prop_map_into().boxed(),

        AlgebraicType::Builtin(BuiltinType::Map(ty)) => generate_map_value(*ty).prop_map_into().boxed(),

        AlgebraicType::Product(ty) => generate_product_value(ty).prop_map_into().boxed(),

        AlgebraicType::Sum(ty) => generate_sum_value(ty).prop_map_into().boxed(),

        AlgebraicType::Ref(_) => unreachable!(),
    }
}

/// Generates a `ProductValue` typed at `ty`.
pub fn generate_product_value(ty: ProductType) -> impl Strategy<Value = ProductValue> {
    ty.elements
        .into_iter()
        .map(|elem| generate_algebraic_value(elem.algebraic_type))
        .collect::<Vec<_>>()
        .prop_map(|elements| ProductValue { elements })
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
        0..=16,
    )
    .prop_map(|entries| entries.into_iter().collect())
}

/// Generates an array value given an element generator `gen_elem`.
fn generate_array_of<S>(gen_elem: S) -> BoxedStrategy<ArrayValue>
where
    S: Strategy + 'static,
    Vec<S::Value>: 'static + Into<ArrayValue>,
{
    vec(gen_elem, 0..=16).prop_map_into().boxed()
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
        AlgebraicType::F32 => generate_array_of(any::<f32>().prop_map_into::<F32>()),
        AlgebraicType::F64 => generate_array_of(any::<f64>().prop_map_into::<F64>()),
        AlgebraicType::String => generate_array_of(any::<String>()),
        AlgebraicType::Product(ty) => generate_array_of(generate_product_value(ty)),
        AlgebraicType::Sum(ty) => generate_array_of(generate_sum_value(ty)),
        AlgebraicType::Builtin(BuiltinType::Array(ty)) => generate_array_of(generate_array_value(*ty.elem_ty)),
        AlgebraicType::Builtin(BuiltinType::Map(ty)) => generate_array_of(generate_map_value(*ty)),
        AlgebraicType::Ref(_) => unreachable!(),
    }
}

/// Generates a row type `ty` and a row value typed at `ty`.
pub fn generate_typed_row() -> impl Strategy<Value = (ProductType, ProductValue)> {
    generate_row_type(0..=16).prop_flat_map(|ty| (Just(ty.clone()), generate_product_value(ty)))
}
