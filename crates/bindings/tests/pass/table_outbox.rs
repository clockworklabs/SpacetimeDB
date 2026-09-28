#[spacetimedb::reducer]
fn receive_ping(_ctx: &spacetimedb::ReducerContext, _note: String) {}

#[spacetimedb::reducer]
fn receive_position(_ctx: &spacetimedb::ReducerContext, _x: u32, _y: u32) {}

#[spacetimedb::reducer]
fn on_position_result(_ctx: &spacetimedb::ReducerContext) {}

#[spacetimedb::table(accessor = ping_outbox, outbox(receive_ping))]
struct PingOutbox {
    #[primary_key]
    #[auto_inc]
    msg_id: u64,
    target: spacetimedb::Identity,
    note: String,
}

#[spacetimedb::table(accessor = position_outbox, outbox(receive_position, on_result = on_position_result))]
struct PositionOutbox {
    #[primary_key]
    #[auto_inc]
    msg_id: u64,
    target: spacetimedb::Identity,
    x: u32,
    y: u32,
}

fn main() {}
