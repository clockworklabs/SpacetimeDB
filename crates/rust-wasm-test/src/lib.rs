use serde::{Deserialize, Deserializer, Serialize};
use spacetimedb_bindings::println;
use spacetimedb_bindings::*;

/*
TODO:
Handle strings
Handle structs
Handle contract parameters supplied from host
Impl reading from the db
Impl schema code-gen
Impl stdb as a server
Impl uploading new contract
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

/*

pub fn _init_() {

}

#[spacetimedb(migrate)]
pub fn migrate() {

}

#[spacetimedb]
pub fn test(arg: TestA, arg2: TestB) {
    println!("foo: {:?}", arg2.foo);

    for i in 0..100 {
        Table::insert(Table { a: i + arg.x as u32, b: 1 + arg.y as u32, c: 2 + arg.z as u32 });
    }

    for row in Table::iter() {
        println!("{:?}", row);
    }
}

// converted to:
*/

#[no_mangle]
pub extern "C" fn _reducer_test(arg_ptr: u32, arg_size: u32) {
    let arg_ptr = arg_ptr as *mut u8;
    let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(arg_ptr, arg_size as usize, arg_size as usize) };
    let arg_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let args = arg_json.as_array().unwrap();
    let arg: TestA = serde_json::from_value(args[0].clone()).unwrap();
    let arg2: TestB = serde_json::from_value(args[1].clone()).unwrap();

    println!("foo: {:?}", arg2.foo);

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
