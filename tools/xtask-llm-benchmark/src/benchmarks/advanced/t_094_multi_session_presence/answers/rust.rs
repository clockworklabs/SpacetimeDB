use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = live_session, public)]
pub struct LiveSession {
    #[primary_key]
    pub connection_id: spacetimedb::ConnectionId,
    pub owner: spacetimedb::Identity,
}

#[table(accessor = online_user, public)]
pub struct OnlineUser {
    #[primary_key]
    pub owner: spacetimedb::Identity,
    pub connections: u64,
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    let connection_id = ctx.connection_id().expect("connection missing");
    let owner = ctx.sender();
    ctx.db.live_session().insert(LiveSession { connection_id, owner });
    if let Some(mut row) = ctx.db.online_user().owner().find(owner) {
        row.connections += 1;
        ctx.db.online_user().owner().update(row);
    } else {
        ctx.db.online_user().insert(OnlineUser { owner, connections: 1 });
    }
}
#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    let connection_id = ctx.connection_id().expect("connection missing");
    if let Some(session) = ctx.db.live_session().connection_id().find(connection_id) {
        ctx.db.live_session().connection_id().delete(connection_id);
        if let Some(mut row) = ctx.db.online_user().owner().find(session.owner) {
            if row.connections == 1 {
                ctx.db.online_user().owner().delete(session.owner);
            } else {
                row.connections -= 1;
                ctx.db.online_user().owner().update(row);
            }
        }
    }
}
