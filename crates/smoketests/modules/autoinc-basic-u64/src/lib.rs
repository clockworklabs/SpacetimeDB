#![allow(non_camel_case_types)]
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person_u64)]
pub struct Person_u64 {
    #[auto_inc]
    key_col: u64,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_u64(ctx: &ReducerContext, name: String, expected_value: u64) {
    let value = ctx.db.person_u64().insert(Person_u64 { key_col: 0, name });
    assert_eq!(value.key_col, expected_value);
}

#[spacetimedb::reducer]
pub fn say_hello_u64(ctx: &ReducerContext) {
    for person in ctx.db.person_u64().iter() {
        log::info!("Hello, {}:{}!", person.key_col, person.name);
    }
    log::info!("Hello, World!");
}
