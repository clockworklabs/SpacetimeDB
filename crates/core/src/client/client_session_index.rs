//! Tracking of client sessions, used to replace pre-existing connections.
//!
//! A client which reconnects automatically sends the same client-generated
//! session id on every connection attempt. Each connection still receives its
//! own [`ConnectionId`] and its own `client_connected` / `client_disconnected`
//! events. The session id only identifies which earlier connection a new one
//! supersedes.
//!
//! A client frequently notices a dropped connection before the server does
//! as the server needs up to its idle timeout to notice an idle peer.
//! Without this index the module would briefly observe two live
//! connections for the same client, and the old connection's
//! `client_disconnected` could run after the new connection's
//! `client_connected`.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use spacetimedb_lib::Identity;

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

/// A session is identified by the client's identity together with the
/// client-generated session id, so a session can only ever be replaced by the
/// client that owns it.
type SessionKey = (Identity, SessionId);

/// The connection currently serving a session.
struct SessionEntry {
    client_id: ClientActorId,
    /// Used to stop the connection's actor when it is superseded.
    ///
    /// Weak so that a connection whose actor has already ended can be dropped
    /// normally rather than being kept alive by this map.
    sender: Weak<ClientConnectionSender>,
}

/// The map of live sessions for one host.
///
/// Maps each live session to the connection currently serving it. The entry is
/// removed when that connection ends, so a session never outlives its
/// connection.
#[derive(Default)]
pub struct ClientSessionIndex {
    sessions: Mutex<HashMap<SessionKey, SessionEntry>>,
}

/// A connection which a newly arriving connection supersedes.
/// Returned by `ClientSessionIndex::claim_session`.
pub struct SupersededConnection {
    /// The client actor matching a session id and identity.
    /// Used by the caller to run the module side disconnect
    /// lifecyle before allowing the new connection's `client_connected` to run.
    pub client_id: ClientActorId,
    /// The superseded connection's sender, if its actor is still alive.
    /// Used to terminate the connection's websocket actor.
    pub sender: Option<Arc<ClientConnectionSender>>,
}

impl ClientSessionIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim `session_id` for `client`, returning the connection it supersedes,
    /// if any.
    ///
    /// The caller must tear that connection down before allowing `client`'s
    /// `client_connected` to run, so that the module never observes two live
    /// connections for one session. The claim takes effect immediately, so a
    /// third connection racing for the same session supersedes `client` rather
    /// than the connection returned here.
    ///
    /// `sender` is registered so that a later connection can stop this
    /// connection's actor. It is `None` before the connection's actor exists,
    /// in which case the entry is registered without one.
    pub fn claim_session(
        &self,
        client: ClientActorId,
        session_id: SessionId,
        sender: Option<&Arc<ClientConnectionSender>>,
    ) -> Option<SupersededConnection> {
        let key = (client.identity, session_id);
        let entry = SessionEntry {
            client_id: client,
            sender: sender.map(Arc::downgrade).unwrap_or_default(),
        };
        let mut sessions = self.sessions.lock().expect("session index poisoned");
        match sessions.entry(key) {
            Entry::Occupied(mut occupied) => {
                let superseded = occupied.insert(entry);
                (superseded.client_id.connection_id != client.connection_id).then(|| SupersededConnection {
                    client_id: superseded.client_id,
                    sender: superseded.sender.upgrade(),
                })
            }
            Entry::Vacant(vacant) => {
                vacant.insert(entry);
                None
            }
        }
    }

    /// Record the sender for the connection currently holding `session_id`.
    ///
    /// Called once the connection's actor exists. Does nothing if the session
    /// has already been claimed by a newer connection.
    pub fn attach_sender(&self, client: ClientActorId, session_id: SessionId, sender: &Arc<ClientConnectionSender>) {
        let key = (client.identity, session_id);
        let mut sessions = self.sessions.lock().expect("session index poisoned");
        if let Some(entry) = sessions.get_mut(&key)
            && entry.client_id.connection_id == client.connection_id
        {
            entry.sender = Arc::downgrade(sender);
        }
    }

    /// Release `session_id` if it is still held by `client`.
    ///
    /// Called when a connection ends. A connection which has already been
    /// superseded no longer holds the session, so it leaves the entry alone:
    /// otherwise a slow teardown would evict its own replacement.
    pub fn release_session(&self, client: ClientActorId, session_id: SessionId) {
        let key = (client.identity, session_id);
        let mut sessions = self.sessions.lock().expect("session index poisoned");
        if let Entry::Occupied(entry) = sessions.entry(key)
            && entry.get().client_id.connection_id == client.connection_id
        {
            entry.remove();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ClientName;
    use spacetimedb_lib::ConnectionId;

    fn client(identity: Identity, connection_id: u128) -> ClientActorId {
        ClientActorId {
            identity,
            connection_id: ConnectionId::from_u128(connection_id),
            name: ClientName(0),
        }
    }

    fn an_identity() -> Identity {
        Identity::from_byte_array([1; 32])
    }

    fn another_identity() -> Identity {
        Identity::from_byte_array([2; 32])
    }

    #[test]
    fn first_connection_supersedes_nothing() {
        let index = ClientSessionIndex::new();
        let session = SessionId::from_u128(7);
        assert!(index.claim_session(client(an_identity(), 1), session, None).is_none());
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn reconnect_supersedes_previous_connection() {
        let index = ClientSessionIndex::new();
        let session = SessionId::from_u128(7);
        index.claim_session(client(an_identity(), 1), session, None);

        let superseded = index.claim_session(client(an_identity(), 2), session, None);

        assert_eq!(
            superseded.map(|s| s.client_id.connection_id),
            Some(ConnectionId::from_u128(1))
        );
        // The session is now held by the new connection, not the old one.
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn different_identity_does_not_supersede() {
        let index = ClientSessionIndex::new();
        let session = SessionId::from_u128(7);
        index.claim_session(client(an_identity(), 1), session, None);

        let superseded = index.claim_session(client(another_identity(), 2), session, None);

        assert!(superseded.is_none());
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn different_session_does_not_supersede() {
        let index = ClientSessionIndex::new();
        index.claim_session(client(an_identity(), 1), SessionId::from_u128(7), None);

        let superseded = index.claim_session(client(an_identity(), 2), SessionId::from_u128(8), None);

        assert!(superseded.is_none());
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn release_removes_the_session() {
        let index = ClientSessionIndex::new();
        let session = SessionId::from_u128(7);
        let connection = client(an_identity(), 1);
        index.claim_session(connection, session, None);

        index.release_session(connection, session);

        assert!(index.is_empty());
    }

    #[test]
    fn superseded_connection_release_does_not_evict_its_replacement() {
        let index = ClientSessionIndex::new();
        let session = SessionId::from_u128(7);
        let old = client(an_identity(), 1);
        let new = client(an_identity(), 2);
        index.claim_session(old, session, None);
        index.claim_session(new, session, None);

        // The old connection tears down after being superseded.
        index.release_session(old, session);

        // The replacement still holds the session.
        assert_eq!(index.len(), 1);
        assert_eq!(
            index
                .claim_session(client(an_identity(), 3), session, None)
                .map(|s| s.client_id.connection_id),
            Some(new.connection_id)
        );
    }

    #[test]
    fn three_way_race_supersedes_the_most_recent_connection() {
        let index = ClientSessionIndex::new();
        let session = SessionId::from_u128(7);
        index.claim_session(client(an_identity(), 1), session, None);

        let second = index.claim_session(client(an_identity(), 2), session, None);
        let third = index.claim_session(client(an_identity(), 3), session, None);

        assert_eq!(
            second.map(|s| s.client_id.connection_id),
            Some(ConnectionId::from_u128(1))
        );
        assert_eq!(
            third.map(|s| s.client_id.connection_id),
            Some(ConnectionId::from_u128(2))
        );
    }
}
