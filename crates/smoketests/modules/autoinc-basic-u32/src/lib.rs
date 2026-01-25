#![allow(non_camel_case_types)]
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person_u32)]
pub struct Person_u32 {
    #[auto_inc]
    key_col: u32,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_u32(ctx: &ReducerContext, name: String, expected_value: u32) {
    let value = ctx.db.person_u32().insert(Person_u32 { key_col: 0, name });
    assert_eq!(value.key_col, expected_value);
}

#[spacetimedb::reducer]
pub fn say_hello_u32(ctx: &ReducerContext) {
    for person in ctx.db.person_u32().iter() {
        log::info!("Hello, {}:{}!", person.key_col, person.name);
    }
    log::info!("Hello, World!");
}
