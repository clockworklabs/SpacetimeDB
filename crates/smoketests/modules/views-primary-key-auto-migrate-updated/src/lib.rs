use spacetimedb::{log, ReducerContext, ViewContext};

#[spacetimedb::table(accessor = player_state, public)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    log::info!("VIEW PRIMARY KEY UPDATE: client disconnected");
}

#[spacetimedb::view(accessor = player, public, primary_key = id)]
pub fn player(ctx: &ViewContext) -> Vec<PlayerState> {
    ctx.db.player_state().level().filter(0u64..).collect()
}
