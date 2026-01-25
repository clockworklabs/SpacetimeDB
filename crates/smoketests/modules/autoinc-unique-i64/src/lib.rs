#![allow(non_camel_case_types)]
use std::error::Error;
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person_i64)]
pub struct Person_i64 {
    #[auto_inc]
    #[unique]
    key_col: i64,
    #[unique]
    name: String,
}

#[spacetimedb::reducer]
pub fn add_new_i64(ctx: &ReducerContext, name: String) -> Result<(), Box<dyn Error>> {
    let value = ctx.db.person_i64().try_insert(Person_i64 { key_col: 0, name })?;
    log::info!("Assigned Value: {} -> {}", value.key_col, value.name);
    Ok(())
}

#[spacetimedb::reducer]
pub fn update_i64(ctx: &ReducerContext, name: String, new_id: i64) {
    ctx.db.person_i64().name().delete(&name);
    let _value = ctx.db.person_i64().insert(Person_i64 { key_col: new_id, name });
}

#[spacetimedb::reducer]
pub fn say_hello_i64(ctx: &ReducerContext) {
    for person in ctx.db.person_i64().iter() {
        log::info!("Hello, {}:{}!", person.key_col, person.name);
    }
    log::info!("Hello, World!");
}
