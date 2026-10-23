mod remote_bindings;

use remote_bindings::{auth, game_world};
use spacetimedb::Table;

#[spacetimedb::table(accessor = world_outbox, outbox(game_world::receive_detailed_ping))]
pub struct WorldOutbox {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: game_world::Identity,
    body: String,
    details: game_world::PingDetails,
}

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
    ctx.db.world_outbox().insert(WorldOutbox {
        id: 0,
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
