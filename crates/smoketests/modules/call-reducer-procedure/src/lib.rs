use spacetimedb::{log, Identity, ProcedureContext, ReducerContext};

#[spacetimedb::table(accessor = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn say_hello(_ctx: &ReducerContext) {
    log::info!("Hello, World!");
}

#[spacetimedb::procedure]
pub fn return_person(_ctx: &mut ProcedureContext) -> Person {
    Person {
        name: "World".to_owned(),
    }
}

#[spacetimedb::reducer]
pub fn say_my_identity(_ctx: &ReducerContext, identity: Identity) {
    log::info!("Hello, {identity}!");
}

#[spacetimedb::procedure]
pub fn return_my_identity(_ctx: &mut ProcedureContext, identity: Identity) -> Identity {
    identity
}
