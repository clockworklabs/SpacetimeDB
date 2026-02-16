use spacetimedb::{Identity, ReducerContext, Table, ViewContext};

#[spacetimedb::table(accessor = player_state)]
pub struct PlayerState {
    #[primary_key]
    identity: Identity,
    #[unique]
    name: String,
}

#[spacetimedb::view(accessor = my_player, public)]
pub fn my_player(ctx: &ViewContext) -> Option<PlayerState> {
    ctx.db.player_state().identity().find(ctx.sender())
}

#[spacetimedb::reducer]
pub fn insert_player(ctx: &ReducerContext, name: String) {
    ctx.db.player_state().insert(PlayerState {
        name,
        identity: ctx.sender(),
    });
}
