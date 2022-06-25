use serde::{Deserialize, Serialize};
use spacetimedb_bindgen::spacetimedb;
use spacetimedb_bindings::println;
use spacetimedb_bindings::*;

#[spacetimedb(table(1))]
pub struct TestA {
    pub x: u32,
    pub y: u32,
    pub z: String,
}

#[derive(Serialize, Deserialize)]
pub struct TestB {
    foo: String,
}

#[spacetimedb(migrate)]
pub fn migrate() {}

#[spacetimedb(reducer)]
pub fn test(arg: TestA, arg2: TestB) {
    println!("bar: {:?}", arg2.foo);

    for i in 0..10 {
        TestA::insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: "Yo".to_owned(),
        });
    }

    let mut row_count = 0;
    for row in TestA::iter().unwrap() {
        let x = &row.elements[2];
        let y: &String = x.as_string().unwrap();
        println!("{:?}", y);
        row_count += 1;
    }

    println!("Row count: {:?}", row_count);
}