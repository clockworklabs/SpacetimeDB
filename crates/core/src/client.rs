use crate::identity::Identity;
use std::fmt;

mod client_connection;
mod client_connection_index;
mod message_handlers;
pub mod messages;

pub use client_connection::{ClientClosed, ClientConnection, ClientConnectionSender, DataMessage, Protocol};
pub use client_connection_index::ClientActorIndex;
pub use message_handlers::MessageHandleError;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct ClientActorId {
    pub identity: Identity,
    pub name: ClientName,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct ClientName(pub u64);

impl fmt::Display for ClientActorId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ClientActorId({}/{})", self.identity.to_hex(), self.name.0)
    }
}
