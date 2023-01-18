#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test is designed to test the format of the output of sql queries"
        exit
fi

set -euox pipefail

source "./.test/lib.include"

create_project
cat > "${PROJECT_PATH}/src/lib.rs" <<EOF
use spacetimedb::{spacetimedb, Hash};

#[spacetimedb(tuple)]
struct TupleType {
    a_b: bool,
    a_i8: i8,
    a_i16: i16,
    a_i32: i32,
    a_i64: i64,
    a_i128: i128,
    a_u8: u8,
    a_u16: u16,
    a_u32: u32,
    a_u64: u64,
    a_u128: u128,
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
    a_u8: u8,
    a_u16: u16,
    a_u32: u32,
    a_u64: u64,
    a_u128: u128,
    a_f32: f32,
    a_f64: f64,
    a_str: String,
    a_bytes: Vec<u8>,
    a_tuple: TupleType,
}

#[spacetimedb(reducer)]
pub fn test(_sender: Hash, _timestamp: u64) {
    BuiltIn::insert(BuiltIn {
        a_b: true,
        a_i8: -25,
        a_i16: -3224,
        a_i32: -23443,
        a_i64: -2344353,
        a_i128: -234434897853,
        a_u8: 105,
        a_u16: 1050,
        a_u32: 83892,
        a_u64: 48937498,
        a_u128: 4378528978889,
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
            a_u8: 105,
            a_u16: 1050,
            a_u32: 83892,
            a_u64: 48937498,
            a_u128: 4378528978889,
            a_f32: 594806.58906,
            a_f64: -3454353.345389043278459,
            a_str: "This is spacetimedb".to_string(),
            a_bytes: vec!(1, 2, 3, 4, 5, 6, 7),
        },
    });
}
EOF

spacetime_publish --project-path "$PROJECT_PATH"
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

# We have to give the database some time to setup our instance
sleep 2

# Calling our database
run_test cargo run call "$ADDRESS" test
run_test cargo run sql "$ADDRESS" "SELECT * FROM BuiltIn"

[ "$(cat "$TEST_OUT" | tail -n 3)" == ' a_b  | a_i8 | a_i16 | a_i32  | a_i64    | a_i128        | a_u8 | a_u16 | a_u32 | a_u64    | a_u128        | a_f32     | a_f64      | a_str               | a_bytes                        | a_tuple                                                                                                                                                                                                                      
------+------+-------+--------+----------+---------------+------+-------+-------+----------+---------------+-----------+------------+---------------------+--------------------------------+------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
 true | -25  | -3224 | -23443 | -2344353 | -234434897853 | 105  | 1050  | 83892 | 48937498 | 4378528978889 | 594806.56 | -3454353.3 | This is spacetimedb | "\x01\x02\x03\x04\x05\x06\x07" | {0: true, 1: -25, 2: -3224, 3: -23443, 4: -2344353, 5: -234434897853, 6: 105, 7: 1050, 8: 83892, 9: 48937498, 10: 4378528978889, 11: 594806.56, 12: -3454353.3, 13: This is spacetimedb, 14: "\x01\x02\x03\x04\x05\x06\x07"} ' ]
