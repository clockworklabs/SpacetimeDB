use spacetimedb_sdk as spacetimedb;

#[spacetimedb::table(accessor = connected, public)]
pub struct Connected {
    pub identity: spacetimedb::Identity,
}
