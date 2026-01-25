#![allow(non_camel_case_types)]
use std::error::Error;
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person_u64)]
pub struct Person_u64 {
    #[auto_inc]
    #[unique]
    key_col: u64,
    #[unique]
    name: String,
}

#[spacetimedb::reducer]
pub fn add_new_u64(ctx: &ReducerContext, name: String) -> Result<(), Box<dyn Error>> {
    let value = ctx.db.person_u64().try_insert(Person_u64 { key_col: 0, name })?;
    log::info!("Assigned Value: {} -> {}", value.key_col, value.name);
    Ok(())
}

#[spacetimedb::reducer]
pub fn update_u64(ctx: &ReducerContext, name: String, new_id: u64) {
    ctx.db.person_u64().name().delete(&name);
    let _value = ctx.db.person_u64().insert(Person_u64 { key_col: new_id, name });
}

#[spacetimedb::reducer]
pub fn say_hello_u64(ctx: &ReducerContext) {
    for person in ctx.db.person_u64().iter() {
        log::info!("Hello, {}:{}!", person.key_col, person.name);
    }
    log::info!("Hello, World!");
}
