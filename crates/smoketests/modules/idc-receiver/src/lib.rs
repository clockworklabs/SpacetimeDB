use spacetimedb::Table;

const MODULE_VERSION: u32 = 1;

#[derive(spacetimedb::SpacetimeType)]
pub struct PingDetails {
    pub labels: Vec<String>,
    pub checkpoints: Vec<PingCheckpoint>,
    pub note: Option<String>,
}

#[derive(spacetimedb::SpacetimeType)]
pub struct PingCheckpoint {
    pub sequence: u32,
    pub state: PingState,
}

#[derive(spacetimedb::SpacetimeType)]
pub enum PingState {
    Ready,
    Deferred(Option<String>),
}

#[spacetimedb::table(accessor = ping_log)]
pub struct PingLog {
    #[primary_key]
    #[auto_inc]
    id: u64,
    sender: spacetimedb::Identity,
    module_version: u32,
    body: String,
}

#[spacetimedb::reducer]
pub fn receive_ping(ctx: &spacetimedb::ReducerContext, body: String) {
    ctx.db.ping_log().insert(PingLog {
        id: 0,
        sender: ctx.sender(),
        module_version: MODULE_VERSION,
        body,
    });
}

#[spacetimedb::table(accessor = detailed_ping_log)]
pub struct DetailedPingLog {
    #[primary_key]
    #[auto_inc]
    id: u64,
    body: String,
    details: PingDetails,
}

#[spacetimedb::reducer]
pub fn receive_detailed_ping(ctx: &spacetimedb::ReducerContext, body: String, details: PingDetails) {
    ctx.db
        .detailed_ping_log()
        .insert(DetailedPingLog { id: 0, body, details });
}
