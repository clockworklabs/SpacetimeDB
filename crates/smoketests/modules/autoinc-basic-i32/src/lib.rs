#![allow(non_camel_case_types)]
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person_i32)]
pub struct Person_i32 {
    #[auto_inc]
    key_col: i32,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_i32(ctx: &ReducerContext, name: String, expected_value: i32) {
    let value = ctx.db.person_i32().insert(Person_i32 { key_col: 0, name });
    assert_eq!(value.key_col, expected_value);
}

#[spacetimedb::reducer]
pub fn say_hello_i32(ctx: &ReducerContext) {
    for person in ctx.db.person_i32().iter() {
        log::info!("Hello, {}:{}!", person.key_col, person.name);
    }
    log::info!("Hello, World!");
}
