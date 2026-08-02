use spacetimedb::ReducerContext;

// Minimal fixture module exercising the `stop` lifecycle reducer.

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {
    log::info!("INIT");
}

#[spacetimedb::reducer(stop)]
pub fn stop(_ctx: &ReducerContext) {
    log::info!("STOP");
}

#[spacetimedb::reducer]
pub fn ping(_ctx: &ReducerContext) {
    log::info!("PING");
}
