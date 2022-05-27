use serde::{Deserialize, Serialize};
use spacetimedb_bindings::println;
use spacetimedb_bindings::*;

/*
#[spacetimedb(table)]
struct Table {
    a: u32,
    b: u32,
    c: u32,
}

impl Table {
    fn insert(table: Table) {

    }
}

#[spacetimedb(migration)]
pub fn migrate() {

}

#[spacetimedb(reducer)]
pub fn test(arg: TestA, arg2: TestB) {
    println!("foo: {:?}", arg2.foo);

    for i in 0..100 {
        Table::insert(Table { a: i + arg.x as u32, b: 1 + arg.y as u32, c: 2 + arg.z as u32 });
        Table::delete_a_eq(4);
        Table::delete_b_eq(4);
        Table::delete_c_eq(4);
    }

    for row in Table::iter() {
        println!("{:?}", row);
    }
}

// converted to:
*/

#[derive(Serialize, Deserialize)]
struct TestA {
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Serialize, Deserialize)]
struct TestB {
    foo: String,
}

#[no_mangle]
pub extern "C" fn __init_database__(_arg_ptr: u32, _arg_size: u32) {
    create_table(
        0,
        vec![
            Column {
                col_id: 0,
                col_type: ColType::U32,
            },
            Column {
                col_id: 1,
                col_type: ColType::U32,
            },
            Column {
                col_id: 2,
                col_type: ColType::U32,
            },
        ],
    );
}

#[no_mangle]
pub extern "C" fn __migrate_database__(_arg_ptr: u32, _arg_size: u32) {
    // User defined
}

#[no_mangle]
pub extern "C" fn __reducer__test(arg_ptr: u32, arg_size: u32) {
    let arg_ptr = arg_ptr as *mut u8;
    let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(arg_ptr, arg_size as usize, arg_size as usize) };
    let arg_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let args = arg_json.as_array().unwrap();
    let arg: TestA = serde_json::from_value(args[0].clone()).unwrap();
    let arg2: TestB = serde_json::from_value(args[1].clone()).unwrap();

    println!("foo: {:?}", arg2.foo);

    for i in 0..100 {
        insert(
            0,
            vec![
                ColValue::U32(i + arg.x as u32),
                ColValue::U32(1 + arg.y as u32),
                ColValue::U32(2 + arg.z as u32),
            ],
        );
    }

    for row in iter(0).unwrap() {
        println!("{:?}", row);
    }
}
