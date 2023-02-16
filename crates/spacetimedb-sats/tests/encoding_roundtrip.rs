use spacetimedb_sats::AlgebraicType;

#[test]
fn type_to_binary_equivalent() {
    check_type(&AlgebraicType::make_meta_type());
}

#[track_caller]
fn check_type(ty: &AlgebraicType) {
    let mut through_value = Vec::new();
    ty.as_value().encode(&mut through_value);
    let mut direct = Vec::new();
    ty.encode(&mut direct);
    assert_eq!(direct, through_value);
}
