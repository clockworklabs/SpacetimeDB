use spacetimedb::ViewContext;

#[derive(Copy, Clone)]
#[spacetimedb::table(name = player_state)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[spacetimedb::view(name = player, public)]
pub fn player(ctx: &ViewContext) -> Option<PlayerState> {
    ctx.db.player_state().id().find(0u64)
}
