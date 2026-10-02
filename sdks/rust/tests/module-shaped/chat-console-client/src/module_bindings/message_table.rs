use spacetimedb_sdk as spacetimedb;

use spacetimedb::{Identity, Timestamp};

#[spacetimedb::table(accessor = message, public)]
pub struct Message {
    pub sender: Identity,
    pub sent: Timestamp,
    pub text: String,
}
