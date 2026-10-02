use spacetimedb_sdk as spacetimedb;

use spacetimedb::Identity;

#[spacetimedb::table(accessor = user, public)]
pub struct User {
    #[primary_key]
    pub identity: Identity,
    pub name: Option<String>,
    pub online: bool,
}
