use spacetimedb_sdk as spacetimedb;

#[spacetimedb::reducer]
pub fn send_message(ctx: &spacetimedb::ReducerContext, text: String) -> Result<(), String>;
