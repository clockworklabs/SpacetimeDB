use crate::identity::Identity;
use std::fmt;

mod client_connection;
mod client_connection_index;
mod message_handlers;
pub mod messages;

pub use client_connection::{
    ClientConfig, ClientConnection, ClientConnectionSender, ClientSendError, DataMessage, Protocol,
};
pub use client_connection_index::ClientActorIndex;
pub use message_handlers::MessageHandleError;
use spacetimedb_lib::ConnectionId;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct ClientActorId {
    pub identity: Identity,
    pub connection_id: ConnectionId,
    pub name: ClientName,
}

impl ClientActorId {
    #[cfg(test)]
    pub fn for_test(identity: Identity) -> Self {
        ClientActorId {
            identity,
            connection_id: ConnectionId::ZERO,
            name: ClientName(0),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct ClientName(pub u64);

impl fmt::Display for ClientActorId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ClientActorId({}@{}/{})",
            self.identity.to_hex(),
            self.connection_id.to_hex(),
            self.name.0
        )
    }
}
