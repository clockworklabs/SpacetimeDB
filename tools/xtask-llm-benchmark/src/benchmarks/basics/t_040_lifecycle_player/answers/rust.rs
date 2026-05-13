use spacetimedb::{reducer, table, Identity, ReducerContext, Table, Timestamp};

#[table(accessor = online_player, public)]
pub struct OnlinePlayer {
    #[primary_key]
    pub identity: Identity,
    pub connected_at: Timestamp,
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    ctx.db.online_player().insert(OnlinePlayer {
        identity: ctx.sender(),
        connected_at: ctx.timestamp,
    });
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    ctx.db.online_player().identity().delete(&ctx.sender());
}
