use spacetimedb::delete_range;
use spacetimedb::println;
use spacetimedb::spacetimedb;
use spacetimedb::{Hash, TypeValue};
use std::time::Duration;

#[spacetimedb(table)]
pub struct TestA {
    pub x: u32,
    pub y: u32,
    pub z: String,
}

#[spacetimedb(tuple)]
pub struct TestB {
    foo: String,
}

// #[spacetimedb(migrate)]
// pub fn migrate() {}

#[spacetimedb(reducer, repeat = 1000ms)]
pub fn repeating_test(timestamp: u64, delta_time: u64) {
    let delta_time = Duration::from_millis(delta_time);
    let timestamp = Duration::from_millis(timestamp);
    println!("Timestamp: {:?}, Delta time: {:?}", timestamp, delta_time);
}

#[spacetimedb(reducer)]
pub fn test(sender: Hash, timestamp: u64, arg: TestA, arg2: TestB) {
    println!("BEGIN");
    println!("sender: {:?}", sender);
    println!("timestamp: {}", timestamp);
    println!("bar: {:?}", arg2.foo);

    for i in 0..10 {
        TestA::insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: "Yo".to_owned(),
        });
    }

    let mut row_count = 0;
    for _row in TestA::iter() {
        row_count += 1;
    }

    println!("Row count before delete: {:?}", row_count);

    delete_range(1, 0, TypeValue::U32(5)..TypeValue::U32(10));

    let mut row_count = 0;
    for _row in TestA::iter() {
        row_count += 1;
    }

    println!("Row count after delete: {:?}", row_count);
    println!("END");
}
