//! Tracking of live client sessions, so that at most one connection serves a
//! session at a time.
//!
//! A client which reconnects automatically sends the same client-generated
//! session id on every connection attempt. Each connection still receives its
//! own [`ConnectionId`] and its own `client_connected` / `client_disconnected`
//! events. The session id only identifies which earlier connection a new one
//! replaces.
//!
//! A client frequently notices a dropped connection before the server does,
//! as the server needs up to its idle timeout to notice an idle peer. When a
//! connection arrives for a session which a live connection still holds, the
//! new connection is refused and the old one is stopped. The session frees up
//! once the old connection is fully closed, including its module-side
//! disconnect, so a retry finds `client_disconnected` already run.
//!
//! [`ConnectionId`]: spacetimedb_lib::ConnectionId

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use spacetimedb_lib::Identity;

use crate::worker_metrics::ClientDisconnectCause;

use super::{ClientActorId, ClientConnectionSender};

/// A client-generated identifier for a logical client session,
/// stable across the reconnects of one client connection object.
///
/// Supplied by the client as the `session_id` query parameter.
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug, PartialOrd, Ord)]
pub struct SessionId(u128);

impl SessionId {
    pub fn from_u128(value: u128) -> Self {
        Self(value)
    }

    pub fn to_u128(self) -> u128 {
        self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// A session belongs to one client on one database, so it can only ever be
/// replaced by the same client on the same database.
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
struct SessionKey {
    database_identity: Identity,
    client_identity: Identity,
    session_id: SessionId,
}

/// The connection holding a session.
struct SessionHolder {
    client: ClientActorId,
    /// Stops the connection's actor when a newer connection arrives.
    ///
    /// Empty while the connection is still running `client_connected` and has
    /// no actor yet. Weak so that the map never keeps a sender alive.
    sender: Option<Weak<ClientConnectionSender>>,
}

/// The session is held by another connection.
#[derive(Debug, PartialEq, Eq)]
pub struct SessionBusy;

/// The live sessions of one host, each mapped to the connection holding it.
#[derive(Default)]
pub struct ClientSessionIndex {
    sessions: Mutex<HashMap<SessionKey, SessionHolder>>,
}

impl ClientSessionIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserve a session for `client`.
    ///
    /// If another connection holds the session, its actor is stopped and
    /// [`SessionBusy`] is returned. The session frees up once that connection
    /// has been torn down, so the caller should refuse `client` and let it
    /// retry.
    ///
    /// The returned reservation releases the session when dropped. The caller
    /// should complete it with [`SessionReservation::establish`] once the
    /// connection has an actor, and drop it only after the connection is fully
    /// closed.
    pub fn try_reserve(
        self: &Arc<Self>,
        database_identity: Identity,
        client: ClientActorId,
        session_id: SessionId,
    ) -> Result<SessionReservation, SessionBusy> {
        let key = SessionKey {
            database_identity,
            client_identity: client.identity,
            session_id,
        };
        let mut sessions = self.sessions.lock().expect("session index poisoned");
        if let Some(holder) = sessions.get(&key) {
            log::debug!(
                "websocket: Connection {} refused, session {session_id} still held by {}",
                client.connection_id,
                holder.client.connection_id,
            );
            if let Some(sender) = holder.sender.as_ref().and_then(Weak::upgrade) {
                sender.kick(ClientDisconnectCause::ConnectionSuperseded);
            }
            return Err(SessionBusy);
        }
        sessions.insert(key, SessionHolder { client, sender: None });
        Ok(SessionReservation {
            index: self.clone(),
            key,
            client,
        })
    }

    /// Remove the session if `client` still holds it.
    fn release(&self, key: SessionKey, client: ClientActorId) {
        let mut sessions = self.sessions.lock().expect("session index poisoned");
        // Connections are told apart by their name, the host's per-connection
        // counter, rather than by their connection id, which a client may repeat.
        if sessions.get(&key).is_some_and(|holder| holder.client.name == client.name) {
            sessions.remove(&key);
        }
    }

    /// The number of live sessions. Intended for tests and diagnostics.
    pub fn len(&self) -> usize {
        self.sessions.lock().expect("session index poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A session held by one connection for its whole lifetime.
///
/// Dropping it releases the session, so it must be dropped only after the
/// connection is fully closed, including its module-side disconnect.
pub struct SessionReservation {
    index: Arc<ClientSessionIndex>,
    key: SessionKey,
    client: ClientActorId,
}

impl SessionReservation {
    /// Record the connection's sender, so that a later connection for the
    /// same session can stop it.
    pub fn establish(&self, sender: &Arc<ClientConnectionSender>) {
        let mut sessions = self.index.sessions.lock().expect("session index poisoned");
        if let Some(holder) = sessions.get_mut(&self.key) {
            holder.sender = Some(Arc::downgrade(sender));
        }
    }
}

impl Drop for SessionReservation {
    fn drop(&mut self) {
        self.index.release(self.key, self.client);
    }
}

#[cfg(test)]
mod tests {
    use super::super::client_connection::DurableOffsetSupply;
    use super::*;
    use crate::client::{ClientConfig, ClientName};
    use crate::host::module_host::NoSuchModule;
    use spacetimedb_durability::DurableOffset;
    use spacetimedb_lib::ConnectionId;

    /// The dummy senders below never wait on durability.
    struct NoDurability;

    impl DurableOffsetSupply for NoDurability {
        fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
            Ok(None)
        }
    }

    fn index() -> Arc<ClientSessionIndex> {
        Arc::new(ClientSessionIndex::new())
    }

    /// A client id whose `name` matches its connection id, as the websocket
    /// handler would assign distinct names to distinct connections.
    fn client(identity: Identity, connection_id: u128) -> ClientActorId {
        ClientActorId {
            identity,
            connection_id: ConnectionId::from_u128(connection_id),
            name: ClientName(connection_id as u64),
        }
    }

    fn sender(client: ClientActorId) -> Arc<ClientConnectionSender> {
        Arc::new(ClientConnectionSender::dummy(
            client,
            ClientConfig::for_test(),
            NoDurability,
        ))
    }

    fn a_database() -> Identity {
        Identity::from_byte_array([9; 32])
    }

    fn another_database() -> Identity {
        Identity::from_byte_array([8; 32])
    }

    fn an_identity() -> Identity {
        Identity::from_byte_array([1; 32])
    }

    fn another_identity() -> Identity {
        Identity::from_byte_array([2; 32])
    }

    fn session() -> SessionId {
        SessionId::from_u128(7)
    }

    /// Reserve a session and establish it, as a connected client does.
    fn connect(
        index: &Arc<ClientSessionIndex>,
        database: Identity,
        client: ClientActorId,
        session: SessionId,
    ) -> (SessionReservation, Arc<ClientConnectionSender>) {
        let reservation = index.try_reserve(database, client, session).expect("session should be free");
        let sender = sender(client);
        reservation.establish(&sender);
        (reservation, sender)
    }

    #[tokio::test]
    async fn first_connection_reserves_the_session() {
        let index = index();

        let (_first, _) = connect(&index, a_database(), client(an_identity(), 1), session());

        assert_eq!(index.len(), 1);
    }

    #[tokio::test]
    async fn reconnect_is_refused_and_stops_the_holder() {
        let index = index();
        let (_first, first_sender) = connect(&index, a_database(), client(an_identity(), 1), session());

        let refused = index.try_reserve(a_database(), client(an_identity(), 2), session());

        assert!(refused.is_err());
        assert!(first_sender.is_cancelled());
        assert_eq!(index.len(), 1);
    }

    /// A client may repeat a connection id across connections, so the holder
    /// is told apart by its name rather than by that id.
    #[tokio::test]
    async fn reconnect_reusing_connection_id_is_refused() {
        let index = index();
        let first = ClientActorId {
            name: ClientName(1),
            ..client(an_identity(), 1)
        };
        let second = ClientActorId {
            name: ClientName(2),
            ..client(an_identity(), 1)
        };
        let (_first, first_sender) = connect(&index, a_database(), first, session());

        assert!(index.try_reserve(a_database(), second, session()).is_err());
        assert!(first_sender.is_cancelled());
    }

    #[tokio::test]
    async fn holder_without_a_sender_still_refuses() {
        let index = index();
        let _first = index
            .try_reserve(a_database(), client(an_identity(), 1), session())
            .unwrap();

        assert!(index.try_reserve(a_database(), client(an_identity(), 2), session()).is_err());
        assert_eq!(index.len(), 1);
    }

    #[tokio::test]
    async fn different_identity_does_not_conflict() {
        let index = index();
        let (_first, first_sender) = connect(&index, a_database(), client(an_identity(), 1), session());

        let (_second, _) = connect(&index, a_database(), client(another_identity(), 2), session());

        assert!(!first_sender.is_cancelled());
        assert_eq!(index.len(), 2);
    }

    #[tokio::test]
    async fn different_session_does_not_conflict() {
        let index = index();
        let (_first, first_sender) = connect(&index, a_database(), client(an_identity(), 1), session());

        let (_second, _) = connect(&index, a_database(), client(an_identity(), 2), SessionId::from_u128(8));

        assert!(!first_sender.is_cancelled());
        assert_eq!(index.len(), 2);
    }

    #[tokio::test]
    async fn different_database_does_not_conflict() {
        let index = index();
        let (_first, first_sender) = connect(&index, a_database(), client(an_identity(), 1), session());

        let (_second, _) = connect(&index, another_database(), client(an_identity(), 2), session());

        assert!(!first_sender.is_cancelled());
        assert_eq!(index.len(), 2);
    }

    #[tokio::test]
    async fn dropping_the_reservation_frees_the_session() {
        let index = index();
        let (first, _) = connect(&index, a_database(), client(an_identity(), 1), session());
        assert!(index.try_reserve(a_database(), client(an_identity(), 2), session()).is_err());

        drop(first);

        assert!(index.is_empty());
        let retry = index.try_reserve(a_database(), client(an_identity(), 2), session());
        assert!(retry.is_ok());
        assert_eq!(index.len(), 1);
    }

    /// A connection which never came to be, because `client_connected`
    /// rejected it, frees the session without ever establishing it.
    #[tokio::test]
    async fn dropping_an_unestablished_reservation_frees_the_session() {
        let index = index();
        let reservation = index
            .try_reserve(a_database(), client(an_identity(), 1), session())
            .unwrap();

        drop(reservation);

        assert!(index.is_empty());
    }
}
