use spacetimedb::{table, ConnectionId, Identity, Timestamp};

#[table(accessor = user_profile, public)]
pub struct UserProfile {
    #[primary_key]
    pub identity: Identity,
    pub display_name: String,
    pub created_at: Timestamp,
}

#[table(
    accessor = connection_presence,
    public,
    index(accessor = by_user, btree(columns = [user_identity]))
)]
pub struct ConnectionPresence {
    #[primary_key]
    pub connection_id: ConnectionId,
    pub user_identity: Identity,
    pub last_heartbeat: Timestamp,
    pub online: bool,
}
