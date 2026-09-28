use spacetimedb::{reducer, table, ConnectionId, Identity, ReducerContext, Table, Timestamp};

#[table(accessor = presence_session, public)]
pub struct PresenceSession {
    #[primary_key]
    pub connection_id: ConnectionId,
    #[index(btree)]
    pub identity: Identity,
    pub connected_at: Timestamp,
}

fn add_session(ctx: &ReducerContext, connection_id: ConnectionId) {
    ctx.db.presence_session().insert(PresenceSession {
        connection_id,
        identity: ctx.sender(),
        connected_at: ctx.timestamp,
    });
}

fn remove_session(ctx: &ReducerContext, connection_id: ConnectionId) {
    ctx.db.presence_session().connection_id().delete(connection_id);
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    add_session(ctx, ctx.connection_id().expect("connection id missing"));
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    remove_session(ctx, ctx.connection_id().expect("connection id missing"));
}

#[reducer]
pub fn exercise_presence(ctx: &ReducerContext) {
    let first = ConnectionId::from_u128(1);
    let second = ConnectionId::from_u128(2);
    add_session(ctx, first);
    add_session(ctx, second);
    remove_session(ctx, first);
}
