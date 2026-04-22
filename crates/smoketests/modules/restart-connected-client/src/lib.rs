use log::info;
use spacetimedb::{ConnectionId, Identity, ReducerContext, Table};

#[spacetimedb::table(accessor = connected_client)]
pub struct ConnectedClient {
    identity: Identity,
    connection_id: ConnectionId,
}

#[spacetimedb::reducer(client_connected)]
fn on_connect(ctx: &ReducerContext) {
    ctx.db.connected_client().insert(ConnectedClient {
        identity: ctx.sender(),
        connection_id: ctx.connection_id().expect("sender connection id unset"),
    });
}

#[spacetimedb::reducer(client_disconnected)]
fn on_disconnect(ctx: &ReducerContext) {
    let sender_identity = &ctx.sender();
    let connection_id = ctx.connection_id();
    let sender_connection_id = connection_id.as_ref().expect("sender connection id unset");
    let match_client =
        |row: &ConnectedClient| &row.identity == sender_identity && &row.connection_id == sender_connection_id;
    if let Some(client) = ctx.db.connected_client().iter().find(match_client) {
        ctx.db.connected_client().delete(client);
    }
}

#[spacetimedb::reducer]
fn print_num_connected(ctx: &ReducerContext) {
    let n = ctx.db.connected_client().count();
    info!("CONNECTED CLIENTS: {n}")
}
