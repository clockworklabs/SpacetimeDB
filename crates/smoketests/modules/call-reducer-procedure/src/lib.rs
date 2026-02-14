use spacetimedb::{log, ProcedureContext, ReducerContext};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn say_hello(_ctx: &ReducerContext) {
    log::info!("Hello, World!");
}

#[spacetimedb::procedure]
pub fn return_person(_ctx: &mut ProcedureContext) -> Person {
   return Person { name: "World".to_owned() };
}
