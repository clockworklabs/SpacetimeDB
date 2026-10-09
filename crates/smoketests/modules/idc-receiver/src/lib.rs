use spacetimedb::Table;

#[spacetimedb::table(accessor = ping_log)]
pub struct PingLog {
    #[primary_key]
    #[auto_inc]
    id: u64,
    sender: spacetimedb::Identity,
    target: spacetimedb::Identity,
    body: String,
}

#[spacetimedb::reducer]
pub fn receive_ping(ctx: &spacetimedb::ReducerContext, target: spacetimedb::Identity, body: String) {
    ctx.db.ping_log().insert(PingLog {
        id: 0,
        sender: ctx.sender(),
        target,
        body,
    });
}
