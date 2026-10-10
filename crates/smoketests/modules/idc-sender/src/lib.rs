mod remote_bindings;

use remote_bindings::game_world;
use spacetimedb::Table;

#[spacetimedb::table(accessor = outbound_ping, outbox(game_world::receive_ping))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: game_world::Identity,
    #[param(name = body)]
    bodypo: String,
}

#[spacetimedb::reducer]
pub fn enqueue_ping(ctx: &spacetimedb::ReducerContext, target: spacetimedb::Identity, body: String) {
    ctx.db.outbound_ping().insert(OutboundPing {
        id: 0,
        target: game_world::Identity(target),
        bodypo: body,
    });
}

#[spacetimedb::table(accessor = ping_result)]
pub struct PingResult {
    #[primary_key]
    id: u64,
    body: String,
    succeeded: bool,
    error: Option<String>,
}

#[spacetimedb::reducer(on_result(self::outbound_ping))]
pub fn on_ping_result(ctx: &spacetimedb::ReducerContext, row: OutboundPing, result: Result<(), String>) {
    ctx.db.ping_result().insert(PingResult {
        id: row.id,
        body: row.bodypo,
        succeeded: result.is_ok(),
        error: result.err(),
    });
}
