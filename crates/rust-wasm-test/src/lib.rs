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

// pub fn test(sender: Hash, timestamp: u64, arg: TestA, arg2: TestB) {
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
    for _row in TestA::iter().unwrap() {
        row_count += 1;
    }
    
    println!("Row count before delete: {:?}", row_count);

    delete_filter(1, |value| {
        let x = *value.elements[0].as_u32().unwrap();
        //let y = *value.elements[0].as_u32().unwrap();

        x == 5
    });
    
    let mut row_count = 0;
    for _row in TestA::iter().unwrap() {
        row_count += 1;
    }

    println!("Row count after delete: {:?}", row_count);
}
