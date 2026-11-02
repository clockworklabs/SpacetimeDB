mod remote_bindings;

use remote_bindings::{auth, game_world};

spacetimedb::outbox!(game_world::receive_detailed_ping);

#[spacetimedb::table(accessor = auth_outbox, outbox(auth::receive_detailed_ping))]
pub struct AuthOutbox {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: auth::Identity,
    body: String,
    details: auth::PingDetails,
}

#[spacetimedb::reducer]
pub fn enqueue_both(ctx: &spacetimedb::ReducerContext, world: game_world::Identity, auth: auth::Identity) {
    ctx.db.receive_detailed_ping_outbox().insert(ReceiveDetailedPingOutbox {
        msg_id: 0,
        target: world,
        body: "world".into(),
        details: game_world::PingDetails {
            labels: vec![],
            checkpoints: vec![],
            note: None,
        },
    });
    ctx.db.auth_outbox().insert(AuthOutbox {
        id: 0,
        target: auth,
        body: "auth".into(),
        details: auth::PingDetails {
            labels: vec![],
            checkpoints: vec![],
            note: None,
        },
    });
}
