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
    payload: String,
}

#[spacetimedb::reducer]
pub fn enqueue_ping(ctx: &spacetimedb::ReducerContext, target: spacetimedb::Identity, body: String) {
    ctx.db.outbound_ping().insert(OutboundPing {
        id: 0,
        target: game_world::Identity(target),
        payload: body,
    });
}
