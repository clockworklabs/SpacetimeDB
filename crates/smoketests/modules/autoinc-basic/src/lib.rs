#![allow(non_camel_case_types)]
use spacetimedb::{log, ReducerContext, Table};

macro_rules! autoinc_basic {
    ($($ty:ident),*) => {
        $(
            paste::paste! {
                #[spacetimedb::table(name = [<person_ $ty>])]
                pub struct [<Person_ $ty>] {
                    #[auto_inc]
                    key_col: $ty,
                    name: String,
                }

                #[spacetimedb::reducer]
                pub fn [<add_ $ty>](ctx: &ReducerContext, name: String, expected_value: $ty) {
                    let value = ctx.db.[<person_ $ty>]().insert([<Person_ $ty>] { key_col: 0, name });
                    assert_eq!(value.key_col, expected_value);
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

autoinc_basic!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128);
