pub mod client_connection;
pub mod client_connection_index;

use crate::hash::Hash;
use std::fmt;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct ClientActorId {
    pub identity: Hash,
    pub name: u64,
}

impl fmt::Display for ClientActorId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ClientActorId({}/{})", self.identity.to_hex(), self.name)
    }
}
