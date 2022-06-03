use serde::{Deserialize, Serialize};
use spacetimedb_bindings::println;
use spacetimedb_bindings::*;
use spacetimedb_bindgen::spacetimedb;

#[spacetimedb(table)]
pub struct TestA {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

#[derive(Serialize, Deserialize)]
pub struct TestB {
    foo: String,
}

#[spacetimedb(migrate)]
pub fn migrate() {

}

#[spacetimedb(reducer)]
pub fn test(arg: TestA, arg2: TestB) {
    println!("foo: {:?}", arg2.foo);

    for i in 0..10 {
        TestA::insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: i + arg.z,
        });
    }

    let mut row_count = 0;
    for _row in iter(0).unwrap() {
        row_count += 1;
    }

    println!("Row count: {:?}", row_count);
}












// #[no_mangle]
// pub extern "C" fn __reducer__test(arg_ptr: u32, arg_size: u32) {
//     let arg_ptr = arg_ptr as *mut u8;
//     let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(arg_ptr, arg_size as usize, arg_size as usize) };
//     let arg_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
//     let args = arg_json.as_array().unwrap();
//     let arg: TestA = serde_json::from_value(args[0].clone()).unwrap();
//     let arg2: TestB = serde_json::from_value(args[1].clone()).unwrap();

//     println!("foo: {:?}", arg2.foo);

//     for i in 0..1 {
//         insert(
//             0,
//             vec![
//                 ColValue::U32(i + arg.x as u32),
//                 ColValue::U32(1 + arg.y as u32),
//                 ColValue::U32(2 + arg.z as u32),
//             ],
//         );
//     }

//     let mut row_count = 0;
//     for _row in iter(0).unwrap() {
//         row_count += 1;
//     }

//     println!("Row count: {:?}", row_count);
// }

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

#[no_mangle]
pub extern "C" fn __create_table__TestA(arg_ptr: u32, arg_size: u32) {
    let arg_ptr = arg_ptr as *mut u8;
    let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(arg_ptr, arg_size as usize, arg_size as usize) };
    let arg_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let args = arg_json.as_array().unwrap();
    let arg: u32 = serde_json::from_value(args[0].clone()).unwrap();

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


*/