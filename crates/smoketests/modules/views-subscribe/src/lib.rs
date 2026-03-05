use spacetimedb::{Identity, ProcedureContext, Query, ReducerContext, Table, ViewContext};

#[spacetimedb::table(accessor = player_state)]
pub struct PlayerState {
    #[primary_key]
    identity: Identity,
    #[unique]
    name: String,
    online: bool,
}

#[spacetimedb::view(accessor = my_player, public)]
pub fn my_player(ctx: &ViewContext) -> Option<PlayerState> {
    ctx.db.player_state().identity().find(ctx.sender())
}

#[spacetimedb::view(accessor = all_players, public)]
pub fn all_players(ctx: &ViewContext) -> impl Query<PlayerState> {
    ctx.from.player_state()
}

#[spacetimedb::view(accessor = online_players, public)]
pub fn online_players(ctx: &ViewContext) -> impl Query<PlayerState> {
    ctx.from.player_state().r#where(|row| row.online)
}

#[spacetimedb::reducer]
pub fn insert_player(ctx: &ReducerContext, name: String) {
    ctx.db.player_state().insert(PlayerState {
        name,
        identity: ctx.sender(),
        online: true,
    });
}

#[spacetimedb::procedure]
pub fn insert_player_proc(ctx: &mut ProcedureContext, name: String) {
    let sender = ctx.sender();
    ctx.with_tx(|tx| {
        tx.db.player_state().insert(PlayerState {
            name: name.clone(),
            identity: sender,
            online: true,
        });
    });
}
