use spacetimedb_sdk as spacetimedb;

#[spacetimedb::reducer]
pub fn set_name(ctx: &spacetimedb::ReducerContext, name: String) -> Result<(), String>;
