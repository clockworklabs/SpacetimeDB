use spacetimedb::sats::{F32, F64};
use spacetimedb::table::TableInternal;
use spacetimedb::AlgebraicValue;

#[spacetimedb::table(accessor = defaults)]
pub struct Defaults {
    pub id: u32,
    #[default(32.5)]
    pub f32_value: f32,
    #[default(64.25)]
    pub f64_value: f64,
}

#[test]
fn float_defaults_use_the_column_type() {
    let defaults = defaults__TableHandle::get_default_col_values();

    assert_eq!(defaults.len(), 2);
    assert_eq!(defaults[0].col_id, 1);
    assert_eq!(defaults[0].value, AlgebraicValue::F32(F32::from(32.5)));
    assert_eq!(defaults[1].col_id, 2);
    assert_eq!(defaults[1].value, AlgebraicValue::F64(F64::from(64.25)));
}
