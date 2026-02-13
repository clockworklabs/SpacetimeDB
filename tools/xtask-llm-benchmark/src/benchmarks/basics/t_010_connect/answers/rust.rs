use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = event)]
pub struct Event {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub kind: String,
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    ctx.db.event().insert(Event { id: 0, kind: "connected".into() });
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    ctx.db.event().insert(Event { id: 0, kind: "disconnected".into() });
}
