use spacetimedb::{log, Identity, ProcedureContext, ReducerContext};

#[spacetimedb::table(accessor = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn say_my_identity(_ctx: &ReducerContext, identity: Identity) {
    log::info!("Hello, {identity}!");
}

#[spacetimedb::procedure]
pub fn return_my_identity(_ctx: &mut ProcedureContext, identity: Identity) -> Identity {
    identity
}
