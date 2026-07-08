use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(accessor = marker, public)]
pub struct Marker {
    id: u32,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.marker().insert(Marker { id: 0 });
}

#[spacetimedb::reducer(client_connected)]
pub fn connected(ctx: &ReducerContext) {
    log::info!("http_cancel_repro: connected {}", ctx.sender());
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(ctx: &ReducerContext) {
    log::info!("http_cancel_repro: disconnected {}", ctx.sender());
}

#[spacetimedb::reducer]
pub fn slow(_ctx: &ReducerContext) {
    log::info!("http_cancel_repro: slow reducer started");
    for i in 0..300_000_000u64 {
        core::hint::black_box(i);
    }
    log::info!("http_cancel_repro: slow reducer finished");
}
