mod remote_bindings;

use remote_bindings::{auth, game_world};
use spacetimedb::Table;

#[spacetimedb::table(accessor = world_message)]
pub struct WorldMessage {
    #[primary_key]
    #[auto_inc]
    id: u64,
    details: game_world::PingDetails,
}

#[spacetimedb::table(accessor = auth_message)]
pub struct AuthMessage {
    #[primary_key]
    #[auto_inc]
    id: u64,
    details: auth::PingDetails,
}

#[spacetimedb::reducer]
pub fn record_both(ctx: &spacetimedb::ReducerContext) {
    ctx.db.world_message().insert(WorldMessage {
        id: 0,
        details: game_world::PingDetails {
            labels: vec!["world".into()],
            checkpoints: vec![],
            note: None,
        },
    });
    ctx.db.auth_message().insert(AuthMessage {
        id: 0,
        details: auth::PingDetails {
            labels: vec!["auth".into()],
            checkpoints: vec![],
            note: None,
        },
    });
}
