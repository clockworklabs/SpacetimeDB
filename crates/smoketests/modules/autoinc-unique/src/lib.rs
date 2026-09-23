#![allow(non_camel_case_types)]
use spacetimedb::{log, ReducerContext, Table};
use std::error::Error;

macro_rules! autoinc_unique {
    ($($ty:ident),*) => {
        $(
            paste::paste! {
                #[spacetimedb::table(accessor = [<person_ $ty>])]
                pub struct [<Person_ $ty>] {
                    #[auto_inc]
                    #[unique]
                    key_col: $ty,
                    #[unique]
                    name: String,
                }

                #[spacetimedb::reducer]
                pub fn [<add_new_ $ty>](ctx: &ReducerContext, name: String) -> Result<(), Box<dyn Error>> {
                    let value = ctx.db.[<person_ $ty>]().try_insert([<Person_ $ty>] { key_col: 0, name })?;
                    log::info!("Assigned Value: {} -> {}", value.key_col, value.name);
                    Ok(())
                }

                #[spacetimedb::reducer]
                pub fn [<update_ $ty>](ctx: &ReducerContext, name: String, new_id: $ty) {
                    ctx.db.[<person_ $ty>]().name().delete(&name);
                    let _value = ctx.db.[<person_ $ty>]().insert([<Person_ $ty>] { key_col: new_id, name });
                }

                #[spacetimedb::reducer]
                pub fn [<say_hello_ $ty>](ctx: &ReducerContext) {
                    for person in ctx.db.[<person_ $ty>]().iter() {
                        log::info!("Hello, {}:{}!", person.key_col, person.name);
                    }
                    log::info!("Hello, World!");
                }
            }
        )*
    };
}

autoinc_unique!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128);

#[spacetimedb::table(accessor = repro, public)]
pub struct Repro {
    #[auto_inc]
    #[unique]
    id: u32,
    #[unique]
    name: String,
}

#[spacetimedb::reducer]
pub fn fill_repro_block(ctx: &ReducerContext) {
    for id in 0..4096 {
        ctx.db.repro().insert(Repro {
            id: 0,
            name: format!("committed-{id}"),
        });
    }
}

#[spacetimedb::reducer]
pub fn rollback_after_repro_alloc(ctx: &ReducerContext) -> Result<(), String> {
    ctx.db.repro().insert(Repro {
        id: 0,
        name: "rolled-back".into(),
    });
    Err("rollback after auto-inc allocation".into())
}

#[spacetimedb::reducer]
pub fn add_repro(ctx: &ReducerContext, name: String) -> Result<(), Box<dyn Error>> {
    ctx.db.repro().try_insert(Repro { id: 0, name })?;
    Ok(())
}
