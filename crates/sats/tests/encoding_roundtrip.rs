#![allow(clippy::arc_with_non_send_sync)]

use proptest::prelude::*;
use proptest::proptest;
use spacetimedb_sats::buffer::DecodeError;
use spacetimedb_sats::builtin_value::{F32, F64};
use spacetimedb_sats::{
    meta_type::MetaType, product, AlgebraicType, AlgebraicValue, BuiltinValue, ProductType, ProductTypeElement,
    ProductValue,
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

fn array_values() -> impl Strategy<Value = AlgebraicValue> {
    prop_oneof![
        prop::collection::vec(0u8..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0i16..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0u16..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0i32..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0u32..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0i64..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0u64..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0i128..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0u128..10, 0..10).prop_map(AlgebraicValue::ArrayOf),
        prop::collection::vec(0..10, 0..10).prop_map(|x| {
            let bools: Vec<_> = x.into_iter().map(|x| x == 0).collect();
            AlgebraicValue::ArrayOf(bools)
        }),
        prop::collection::vec(0i32..10, 0..10).prop_map(|x| {
            let strs: Vec<Box<str>> = x.into_iter().map(|x| x.to_string().into()).collect();
            AlgebraicValue::ArrayOf(strs)
        }),
        prop::collection::vec(0i32..10, 0..10).prop_map(|x| {
            let floats: Vec<_> = x.into_iter().map(|x| F32::from_inner(x as f32)).collect();
            AlgebraicValue::ArrayOf(floats)
        }),
        prop::collection::vec(0i32..10, 0..10).prop_map(|x| {
            let floats: Vec<_> = x.into_iter().map(|x| F64::from_inner(x as f64)).collect();
            AlgebraicValue::ArrayOf(floats)
        }),
    ]
}

fn builtin_values() -> impl Strategy<Value = AlgebraicValue> {
    prop_oneof![
        any::<bool>().prop_map(AlgebraicValue::Bool),
        any::<i8>().prop_map(AlgebraicValue::I8),
        any::<u8>().prop_map(AlgebraicValue::U8),
        any::<i16>().prop_map(AlgebraicValue::I16),
        any::<u16>().prop_map(AlgebraicValue::U16),
        any::<i32>().prop_map(AlgebraicValue::I32),
        any::<u32>().prop_map(AlgebraicValue::U32),
        any::<i64>().prop_map(AlgebraicValue::I64),
        any::<u64>().prop_map(AlgebraicValue::U64),
        any::<i128>().prop_map(AlgebraicValue::I128),
        any::<u128>().prop_map(AlgebraicValue::U128),
        any::<f32>().prop_map(|x| AlgebraicValue::F32(x.into())),
        any::<f64>().prop_map(|x| AlgebraicValue::F64(x.into())),
        "[0-1]+".prop_map(|x| {
            let x = x.into_bytes().into();
            AlgebraicValue::Bytes(x)
        }),
        ".*".prop_map(|x| AlgebraicValue::String(x.into())),
    ]
}

fn algebraic_values() -> impl Strategy<Value = AlgebraicValue> {
    let leaf = builtin_values();
    leaf.prop_recursive(
        8,   // 8 levels deep
        128, // Shoot for maximum size of 128 nodes
        10,  // We put up to 10 items per collection
        |inner| {
            prop_oneof![
                // Take the inner strategy and make the recursive cases.
                array_values(),
                prop::collection::vec(inner.clone(), 0..1).prop_map(|val| {
                    if let Some(x) = val.first().cloned() {
                        AlgebraicValue::OptionSome(x)
                    } else {
                        AlgebraicValue::OptionNone()
                    }
                }),
                prop::collection::btree_map(inner.clone(), inner.clone(), 1..2)
                    .prop_map(|val| { BuiltinValue::Map { val: Box::new(val) }.into() }),
                prop::collection::vec(inner, 0..10).prop_map(|val| {
                    let product = ProductValue::from_iter(val.into_iter());
                    AlgebraicValue::Product(product)
                })
            ]
        },
    )
}

fn round_trip(value: AlgebraicValue) -> Result<(ProductValue, ProductValue), DecodeError> {
    let ty = value.type_of();
    let schema = ProductType::new([ProductTypeElement::new(ty, Some("x".into()))].into());

    let row = product!(value);

    let mut bytes = Vec::new();
    row.encode(&mut bytes);
    ProductValue::decode(&schema, &mut &bytes[..]).map(|x| (x, row))
}

proptest! {
    #[test]
    fn parses_all_builtin_value(enc in builtin_values()) {
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
