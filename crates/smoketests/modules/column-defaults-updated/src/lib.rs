#[spacetimedb::table(accessor = defaults_test_table, public)]
pub struct DefaultsTestTable {
    pub id: u32,
    #[default(true)]
    pub bool_value: bool,
    #[default(-8)]
    pub i8_value: i8,
    #[default(8)]
    pub u8_value: u8,
    #[default(-16)]
    pub i16_value: i16,
    #[default(16)]
    pub u16_value: u16,
    #[default(-32)]
    pub i32_value: i32,
    #[default(32)]
    pub u32_value: u32,
    #[default(-64)]
    pub i64_value: i64,
    #[default(64)]
    pub u64_value: u64,
    #[default(32.5)]
    pub f32_positive_value: f32,
    #[default(-32.5)]
    pub f32_negative_value: f32,
    #[default(64.25)]
    pub f64_positive_value: f64,
    #[default(-64.25)]
    pub f64_negative_value: f64,
    #[default("default string")]
    pub string_value: String,
}
