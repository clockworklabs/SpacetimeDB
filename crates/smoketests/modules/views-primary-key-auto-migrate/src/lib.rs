use spacetimedb::ViewContext;

#[spacetimedb::table(accessor = player_state, public)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[spacetimedb::view(accessor = player, public)]
pub fn player(ctx: &ViewContext) -> Vec<PlayerState> {
    ctx.db.player_state().level().filter(0u64..).collect()
}
