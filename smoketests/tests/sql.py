from .. import Smoketest

class SqlFormat(Smoketest):
    MODULE_CODE = """
use spacetimedb::{spacetimedb, Identity, sats::{i256, u256}};

#[derive(spacetimedb::SpacetimeType)]
pub struct TupleType {
    a_b: bool,
    a_i8: i8,
    a_i16: i16,
    a_i32: i32,
    a_i64: i64,
    a_i128: i128,
    a_i256: i256,
    a_u8: u8,
    a_u16: u16,
    a_u32: u32,
    a_u64: u64,
    a_u128: u128,
    a_u256: u256,
    a_f32: f32,
    a_f64: f64,
    a_str: String,
    a_bytes: Vec<u8>,
}

#[spacetimedb(table)]
pub struct BuiltIn {
    a_b: bool,
    a_i8: i8,
    a_i16: i16,
    a_i32: i32,
    a_i64: i64,
    a_i128: i128,
    a_i256: i256,
    a_u8: u8,
    a_u16: u16,
    a_u32: u32,
    a_u64: u64,
    a_u128: u128,
    a_u256: u256,
    a_f32: f32,
    a_f64: f64,
    a_str: String,
    a_bytes: Vec<u8>,
    a_tuple: TupleType,
}

#[spacetimedb(reducer)]
pub fn test() {
    BuiltIn::insert(BuiltIn {
        a_b: true,
        a_i8: -25,
        a_i16: -3224,
        a_i32: -23443,
        a_i64: -2344353,
        a_i128: -234434897853,
        a_i256: (-234434897853i128).into(),
        a_u8: 105,
        a_u16: 1050,
        a_u32: 83892,
        a_u64: 48937498,
        a_u128: 4378528978889,
        a_u256: 4378528978889u128.into(),
        a_f32: 594806.58906,
        a_f64: -3454353.345389043278459,
        a_str: "This is spacetimedb".to_string(),
        a_bytes: vec!(1, 2, 3, 4, 5, 6, 7),
        a_tuple: TupleType {
            a_b: true,
            a_i8: -25,
            a_i16: -3224,
            a_i32: -23443,
            a_i64: -2344353,
            a_i128: -234434897853,
            a_i256: (-234434897853i128).into(),
            a_u8: 105,
            a_u16: 1050,
            a_u32: 83892,
            a_u64: 48937498,
            a_u128: 4378528978889,
            a_u256: 4378528978889u128.into(),
            a_f32: 594806.58906,
            a_f64: -3454353.345389043278459,
            a_str: "This is spacetimedb".to_string(),
            a_bytes: vec!(1, 2, 3, 4, 5, 6, 7),
        },
    });
}
"""
    def test_sql_format(self):
        """This test is designed to test the format of the output of sql queries"""

        self.call("test")
        sql_out = self.spacetime("sql", self.address, "SELECT * FROM BuiltIn")
        # The spaces after the last column are important
        self.assertMultiLineEqual(sql_out, """\
 a_b  | a_i8 | a_i16 | a_i32  | a_i64    | a_i128        | a_i256        | a_u8 | a_u16 | a_u32 | a_u64    | a_u128        | a_u128        | a_f32     | a_f64              | a_str                 | a_bytes          | a_tuple
------+------+-------+--------+----------+---------------+---------------+------+-------+-------+----------+---------------+---------------+-----------+--------------------+-----------------------+------------------+-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
 true | -25  | -3224 | -23443 | -2344353 | -234434897853 | -234434897853 | 105  | 1050  | 83892 | 48937498 | 4378528978889 | 4378528978889 | 594806.56 | -3454353.345389043 | "This is spacetimedb" | 0x01020304050607 | (a_b = true, a_i8 = -25, a_i16 = -3224, a_i32 = -23443, a_i64 = -2344353, a_i128 = -234434897853, a_i256 = -234434897853, a_u8 = 105, a_u16 = 1050, a_u32 = 83892, a_u64 = 48937498, a_u128 = 4378528978889, a_u256 = 4378528978889, a_f32 = 594806.56, a_f64 = -3454353.345389043, a_str = "This is spacetimedb", a_bytes = 0x01020304050607)
""")
