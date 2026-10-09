#[spacetimedb::reducer]
pub fn receive_ping(_ctx: &spacetimedb::ReducerContext, _target: spacetimedb::Identity, _body: String) {}
