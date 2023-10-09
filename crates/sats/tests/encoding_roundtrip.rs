#![allow(clippy::arc_with_non_send_sync)]

use proptest::collection::{btree_map, vec};
use proptest::prelude::*;
use proptest::proptest;
use spacetimedb_sats::{
    algebraic_value::{F32, F64},
    buffer::DecodeError,
    meta_type::MetaType,
    product, AlgebraicType, AlgebraicValue, ArrayValue, ProductType, ProductValue,
};

#[test]
fn type_to_binary_equivalent() {
    check_type(&AlgebraicType::meta_type());
}

#[track_caller]
fn check_type(ty: &AlgebraicType) {
    let mut through_value = Vec::new();
    ty.as_value().encode(&mut through_value);
    let mut direct = Vec::new();
    ty.encode(&mut direct);
    assert_eq!(direct, through_value);
}

fn map_vec<T, U>(vec: Vec<T>, map: impl Fn(T) -> U) -> Vec<U> {
    vec.into_iter().map(map).collect()
}

fn array_value<T>(vec: Vec<T>) -> AlgebraicValue
where
    ArrayValue: From<Vec<T>>,
{
    AlgebraicValue::Array(vec.into())
}

fn array_values() -> impl Strategy<Value = AlgebraicValue> {
    prop_oneof![
        vec(0u8..10, 0..10).prop_map(array_value),
        vec(0i16..10, 0..10).prop_map(array_value),
        vec(0u16..10, 0..10).prop_map(array_value),
        vec(0i32..10, 0..10).prop_map(array_value),
        vec(0u32..10, 0..10).prop_map(array_value),
        vec(0i64..10, 0..10).prop_map(array_value),
        vec(0u64..10, 0..10).prop_map(array_value),
        vec(0i128..10, 0..10).prop_map(array_value),
        vec(0u128..10, 0..10).prop_map(array_value),
        vec(0..10, 0..10).prop_map(|v| array_value(map_vec(v, |x| x == 0))),
        vec(0i32..10, 0..10).prop_map(|v| array_value(map_vec(v, |x| x.to_string()))),
        vec(0i32..10, 0..10).prop_map(|v| array_value(map_vec(v, |x| F32::from_inner(x as f32)))),
        vec(0i32..10, 0..10).prop_map(|v| array_value(map_vec(v, |x| F64::from_inner(x as f64)))),
    ]
}

fn leaf_values() -> impl Strategy<Value = AlgebraicValue> {
    prop_oneof![
        any::<bool>().prop_map(Into::into),
        any::<i8>().prop_map(Into::into),
        any::<u8>().prop_map(Into::into),
        any::<i16>().prop_map(Into::into),
        any::<u16>().prop_map(Into::into),
        any::<i32>().prop_map(Into::into),
        any::<u32>().prop_map(Into::into),
        any::<i64>().prop_map(Into::into),
        any::<u64>().prop_map(Into::into),
        any::<i128>().prop_map(Into::into),
        any::<u128>().prop_map(Into::into),
        any::<f32>().prop_map(Into::into),
        any::<f64>().prop_map(Into::into),
        "[0-1]+".prop_map(|x| array_value(x.into_bytes())),
        ".*".prop_map(AlgebraicValue::String),
    ]
}

fn algebraic_values() -> impl Strategy<Value = AlgebraicValue> {
    let leaf = leaf_values();
    leaf.prop_recursive(
        8,   // 8 levels deep
        128, // Shoot for maximum size of 128 nodes
        10,  // We put up to 10 items per collection
        |inner| {
            prop_oneof![
                // Take the inner strategy and make the recursive cases.
                array_values(),
                vec(inner.clone(), 0..1).prop_map(|val| val.first().cloned().into()),
                btree_map(inner.clone(), inner.clone(), 1..2).prop_map(AlgebraicValue::map),
                vec(inner, 0..10).prop_map(AlgebraicValue::product)
            ]
        },
    )
}

fn round_trip(value: AlgebraicValue) -> Result<(ProductValue, ProductValue), DecodeError> {
    let ty = value.type_of();
    let schema = ProductType::from([("x", ty)]);

    let row = product!(value);

    let mut bytes = Vec::new();
    row.encode(&mut bytes);
    ProductValue::decode(&schema, &mut &bytes[..]).map(|x| (x, row))
}

proptest! {
    #[test]
    fn parses_all_builtin_value(enc in leaf_values()) {
        let parsed = round_trip(enc);
        prop_assert!(parsed.is_ok());
        let (parsed, original) = parsed.unwrap();
        prop_assert_eq!(parsed, original);
    }

    //TODO: Remove the `ignore` when the encoding get fixed
    #[test]
    #[ignore]
    fn parses_all_values(enc in algebraic_values()) {
        let parsed = round_trip(enc);
        prop_assert!(parsed.is_ok());
        let (parsed, original) = parsed.unwrap();
        prop_assert_eq!(original,parsed, "Original vs Parsed");
    }
}
