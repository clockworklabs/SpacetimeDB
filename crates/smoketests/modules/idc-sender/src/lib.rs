mod remote_bindings;

use remote_bindings::game_world;
use spacetimedb::Table;

const MODULE_VERSION: u32 = 1;

#[spacetimedb::table(accessor = ping_log)]
pub struct PingLog {
    #[primary_key]
    #[auto_inc]
    id: u64,
    sender: spacetimedb::Identity,
    body: String,
}

#[spacetimedb::reducer]
pub fn receive_ping(ctx: &spacetimedb::ReducerContext, body: String) {
    ctx.db.ping_log().insert(PingLog {
        id: 0,
        sender: ctx.sender(),
        body,
    });
}

#[spacetimedb::table(accessor = outbound_ping, outbox(game_world::receive_ping))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    message_number: u64,
    #[target]
    destination: game_world::Identity,
    #[param(name = body)]
    bodypo: String,
}

#[spacetimedb::reducer]
pub fn enqueue_ping(ctx: &spacetimedb::ReducerContext, target: game_world::Identity, body: String) {
    ctx.db.outbound_ping().insert(OutboundPing {
        message_number: 0,
        destination: target,
        bodypo: body,
    });
}

#[spacetimedb::reducer]
pub fn enqueue_pings(ctx: &spacetimedb::ReducerContext, target: game_world::Identity, bodies: Vec<String>) {
    for body in bodies {
        enqueue_ping(ctx, target, body);
    }
}

#[spacetimedb::table(accessor = ping_result)]
pub struct PingResult {
    #[primary_key]
    id: u64,
    #[auto_inc]
    callback_order: u64,
    module_version: u32,
    body: String,
    succeeded: bool,
    error: Option<String>,
}

#[spacetimedb::reducer(on_result(self::outbound_ping))]
pub fn on_ping_result(ctx: &spacetimedb::ReducerContext, row: OutboundPing, result: Result<(), String>) {
    ctx.db.ping_result().insert(PingResult {
        id: row.message_number,
        callback_order: 0,
        module_version: MODULE_VERSION,
        body: row.bodypo,
        succeeded: result.is_ok(),
        error: result.err(),
    });
}

#[spacetimedb::table(accessor = outbound_detailed_ping, outbox(game_world::receive_detailed_ping))]
pub struct OutboundDetailedPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: game_world::Identity,
    body: String,
    details: game_world::PingDetails,
}

#[spacetimedb::reducer]
pub fn enqueue_detailed_ping(ctx: &spacetimedb::ReducerContext, target: game_world::Identity, body: String) {
    ctx.db.outbound_detailed_ping().insert(OutboundDetailedPing {
        id: 0,
        target,
        body,
        details: game_world::PingDetails {
            labels: vec!["nested".into(), "idc".into()],
            checkpoints: vec![
                game_world::PingCheckpoint {
                    sequence: 1,
                    state: game_world::PingState::Ready,
                },
                game_world::PingCheckpoint {
                    sequence: 2,
                    state: game_world::PingState::Deferred(Some("waiting".into())),
                },
                game_world::PingCheckpoint {
                    sequence: 3,
                    state: game_world::PingState::Deferred(None),
                },
            ],
            note: Some("optional-note".into()),
        },
    });
}

#[spacetimedb::table(accessor = detailed_ping_result)]
pub struct DetailedPingResult {
    #[primary_key]
    id: u64,
    body: String,
    details: game_world::PingDetails,
}

#[spacetimedb::reducer(on_result(self::outbound_detailed_ping))]
pub fn on_detailed_ping_result(
    ctx: &spacetimedb::ReducerContext,
    row: OutboundDetailedPing,
    _result: Result<(), String>,
) {
    ctx.db.detailed_ping_result().insert(DetailedPingResult {
        id: row.id,
        body: row.body,
        details: row.details,
    });
}
