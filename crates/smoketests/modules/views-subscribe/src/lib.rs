use spacetimedb::{Identity, ProcedureContext, ReducerContext, Table, ViewContext};

#[spacetimedb::table(name = player_state)]
pub struct PlayerState {
    #[primary_key]
    identity: Identity,
    #[unique]
    name: String,
}

#[spacetimedb::view(name = my_player, public)]
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

#[spacetimedb::procedure]
pub fn insert_player_proc(ctx: &mut ProcedureContext, name: String) {
    let sender = ctx.sender();
    ctx.with_tx(|tx| {
        tx.db.player_state().insert(PlayerState {
            name: name.clone(),
            identity: sender,
        });
    });
}
