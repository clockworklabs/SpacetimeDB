use spacetimedb_sdk as spacetimedb;

#[spacetimedb::table(accessor = disconnected, public)]
pub struct Disconnected {
    pub identity: spacetimedb::Identity,
}
