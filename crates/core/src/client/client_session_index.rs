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

use std::collections::hash_map::{Entry, OccupiedEntry};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, Weak};

use spacetimedb_lib::Identity;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

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

/// A session is identified by the database and the client's identity together
/// with the client-generated session id, so a session can only ever be replaced
/// by the same client on the same database.
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
struct SessionKey {
    database_identity: Identity,
    client_identity: Identity,
    session_id: SessionId,
}

/// The connection currently serving a session.
struct SessionHolder {
    client_id: ClientActorId,
    /// Used to stop the connection's actor when it is superseded.
    ///
    /// Weak so that a connection whose actor has already ended can be dropped
    /// normally rather than being kept alive by this map. Empty until the
    /// claiming connection is established, see [`SessionClaim::attach_sender`].
    sender: Weak<ClientConnectionSender>,
}

/// One session's holder, behind the lock which serializes handovers of it.
///
/// The lock is held for the whole of a handover: from the moment a connection
/// claims the session until that connection is established, or gives up.
/// A connection claiming a session whose handover is still in flight waits
/// here, so it never finds a holder whose connection does not exist yet and
/// therefore cannot be stopped.
type SessionSlot = AsyncMutex<Option<SessionHolder>>;

/// The map of live sessions for one host.
///
/// Maps each live session to the connection currently serving it. The entry is
/// removed when that connection ends, so a session never outlives its
/// connection.
#[derive(Default)]
pub struct ClientSessionIndex {
    sessions: Mutex<HashMap<SessionKey, Arc<SessionSlot>>>,
}

impl ClientSessionIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim a session for `client`, tearing down the connection it supersedes.
    ///
    /// Returns once that connection is fully closed: its actor has been
    /// stopped, and `teardown`, which runs the module-side disconnect, has run
    /// to completion. The caller may therefore run `client_connected` for
    /// `client` as soon as this returns, and the module observes the two
    /// connections' lifecycle events in order.
    ///
    /// The returned [`SessionClaim`] holds the session for the rest of the
    /// handover. Another connection claiming the same session waits until the
    /// claim is completed with [`SessionClaim::attach_sender`] or dropped,
    /// so every claim finds a connection which it can actually stop.
    pub async fn claim_session<F, Fut>(
        self: &Arc<Self>,
        database_identity: Identity,
        client: ClientActorId,
        session_id: SessionId,
        teardown: F,
    ) -> SessionClaim
    where
        F: FnOnce(ClientActorId) -> Fut,
        Fut: Future<Output = ()>,
    {
        let key = SessionKey {
            database_identity,
            client_identity: client.identity,
            session_id,
        };
        let slot = {
            let mut sessions = self.sessions.lock().expect("session index poisoned");
            sessions.entry(key).or_default().clone()
        };

        // Wait out any handover of this session which is still in flight.
        let mut holder = slot.lock_owned().await;
        let superseded = holder.replace(SessionHolder {
            client_id: client,
            sender: Weak::new(),
        });

        // Connections are told apart by their name, the host's per-connection
        // counter, rather than by their connection id, which a client may
        // repeat across connections.
        if let Some(superseded) = superseded.filter(|superseded| superseded.client_id.name != client.name) {
            log::debug!(
                "websocket: Connection {} supersedes {} for session {session_id}",
                client.connection_id,
                superseded.client_id.connection_id,
            );
            if let Some(sender) = superseded.sender.upgrade() {
                sender.kick(ClientDisconnectCause::ConnectionSuperseded);
            }
            teardown(superseded.client_id).await;
        }

        SessionClaim {
            index: self.clone(),
            key,
            holder: Some(holder),
        }
    }

    /// Release a session if it is still held by `client`.
    ///
    /// Called when a connection ends. A connection which has already been
    /// superseded no longer holds the session, so it leaves the entry alone:
    /// otherwise a slow teardown would evict its own replacement.
    pub fn release_session(&self, database_identity: Identity, client: ClientActorId, session_id: SessionId) {
        let key = SessionKey {
            database_identity,
            client_identity: client.identity,
            session_id,
        };
        let mut sessions = self.sessions.lock().expect("session index poisoned");
        let Entry::Occupied(entry) = sessions.entry(key) else {
            return;
        };
        // This runs while a connection is being dropped, so it must not block.
        // Failing to take the lock means a handover of this session is in
        // flight: either another connection's, in which case this connection
        // no longer holds the session and there is nothing to release, or this
        // connection's own, whose actor ended before it was established. The
        // latter leaves the entry behind, holding a sender which can no longer
        // be upgraded, until the next claim of the session replaces it.
        let Ok(mut holder) = entry.get().clone().try_lock_owned() else {
            return;
        };
        if holder.as_ref().is_some_and(|held| held.client_id.name == client.name) {
            *holder = None;
        }
        let is_vacant = holder.is_none();
        drop(holder);
        if is_vacant {
            Self::prune(entry);
        }
    }

    /// Remove a session which no connection holds and none is claiming.
    ///
    /// A connection waiting to claim the session holds a reference to the slot,
    /// so a strong count of one means the map holds the only reference and the
    /// entry can go. Anything else would leave a claimant waiting on a slot no
    /// longer reachable from the map, which a later claimant would not find.
    fn prune(entry: OccupiedEntry<'_, SessionKey, Arc<SessionSlot>>) {
        if Arc::strong_count(entry.get()) == 1 {
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

/// A session held for the duration of one handover.
///
/// Returned by [`ClientSessionIndex::claim_session`]. Completed by
/// [`SessionClaim::attach_sender`] once the claiming connection exists;
/// dropping it without that gives the session up again, for a connection which
/// never came to be.
pub struct SessionClaim {
    index: Arc<ClientSessionIndex>,
    key: SessionKey,
    /// `Some` until the claim is completed or dropped.
    holder: Option<OwnedMutexGuard<Option<SessionHolder>>>,
}

impl SessionClaim {
    /// Complete the handover, recording the now-established connection's sender
    /// so that a later connection can stop it.
    ///
    /// Holding the claim is what proves the session is still this connection's,
    /// so no ownership check is needed here.
    pub fn attach_sender(mut self, sender: &Arc<ClientConnectionSender>) {
        let mut holder = self.holder.take().expect("a claim is completed at most once");
        if let Some(held) = holder.as_mut() {
            held.sender = Arc::downgrade(sender);
        }
    }
}

impl Drop for SessionClaim {
    fn drop(&mut self) {
        // Completed claims took the guard in `attach_sender`, leaving the
        // session held by the connection which is now serving it.
        let Some(mut holder) = self.holder.take() else {
            return;
        };
        *holder = None;
        drop(holder);

        let mut sessions = self.index.sessions.lock().expect("session index poisoned");
        if let Entry::Occupied(entry) = sessions.entry(self.key) {
            ClientSessionIndex::prune(entry);
        }
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
    use std::sync::atomic::{AtomicBool, Ordering};

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

    /// A client id whose `name`, the host's per-connection counter, matches its
    /// connection id, so that tests naming distinct connections get distinct
    /// names as the websocket handler would assign them.
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

    /// Claim a session, recording which connection was torn down, if any.
    async fn claim(
        index: &Arc<ClientSessionIndex>,
        database: Identity,
        client: ClientActorId,
        session: SessionId,
    ) -> (SessionClaim, Option<ClientActorId>) {
        let mut superseded = None;
        let claim = index
            .claim_session(database, client, session, async |old| superseded = Some(old))
            .await;
        (claim, superseded)
    }

    /// Claim a session and complete the handover, as an established connection
    /// does.
    async fn connect(
        index: &Arc<ClientSessionIndex>,
        database: Identity,
        client: ClientActorId,
        session: SessionId,
    ) -> (Arc<ClientConnectionSender>, Option<ClientActorId>) {
        let (claim, superseded) = claim(index, database, client, session).await;
        let sender = sender(client);
        claim.attach_sender(&sender);
        (sender, superseded)
    }

    #[tokio::test]
    async fn first_connection_supersedes_nothing() {
        let index = index();
        let session = SessionId::from_u128(7);

        let (_sender, superseded) = connect(&index, a_database(), client(an_identity(), 1), session).await;

        assert!(superseded.is_none());
        assert_eq!(index.len(), 1);
    }

    #[tokio::test]
    async fn reconnect_supersedes_previous_connection() {
        let index = index();
        let session = SessionId::from_u128(7);
        let (first, _) = connect(&index, a_database(), client(an_identity(), 1), session).await;

        let (_second, superseded) = connect(&index, a_database(), client(an_identity(), 2), session).await;

        assert_eq!(
            superseded.map(|old| old.connection_id),
            Some(ConnectionId::from_u128(1))
        );
        // The superseded connection's actor is stopped.
        assert!(first.is_cancelled());
        // The session is now held by the new connection, not the old one.
        assert_eq!(index.len(), 1);
    }

    /// A client may repeat a connection id across connections, so the session
    /// is handed over on the connection's name rather than on that id.
    #[tokio::test]
    async fn reconnect_reusing_connection_id_supersedes() {
        let index = index();
        let session = SessionId::from_u128(7);
        let first = ClientActorId {
            name: ClientName(1),
            ..client(an_identity(), 1)
        };
        let second = ClientActorId {
            name: ClientName(2),
            ..client(an_identity(), 1)
        };
        let (first_sender, _) = connect(&index, a_database(), first, session).await;

        let (_second_sender, superseded) = connect(&index, a_database(), second, session).await;

        assert_eq!(superseded.map(|old| old.name), Some(ClientName(1)));
        assert!(first_sender.is_cancelled());
        assert_eq!(index.len(), 1);
    }

    #[tokio::test]
    async fn different_identity_does_not_supersede() {
        let index = index();
        let session = SessionId::from_u128(7);
        let (_first, _) = connect(&index, a_database(), client(an_identity(), 1), session).await;

        let (_second, superseded) = connect(&index, a_database(), client(another_identity(), 2), session).await;

        assert!(superseded.is_none());
        assert_eq!(index.len(), 2);
    }

    #[tokio::test]
    async fn different_session_does_not_supersede() {
        let index = index();
        let (_first, _) = connect(&index, a_database(), client(an_identity(), 1), SessionId::from_u128(7)).await;

        let (_second, superseded) =
            connect(&index, a_database(), client(an_identity(), 2), SessionId::from_u128(8)).await;

        assert!(superseded.is_none());
        assert_eq!(index.len(), 2);
    }

    /// A session belongs to one database, so a connection to another database
    /// never tears down this one, whose module knows nothing about it.
    #[tokio::test]
    async fn different_database_does_not_supersede() {
        let index = index();
        let session = SessionId::from_u128(7);
        let (first, _) = connect(&index, a_database(), client(an_identity(), 1), session).await;

        let (_second, superseded) = connect(&index, another_database(), client(an_identity(), 2), session).await;

        assert!(superseded.is_none());
        assert!(!first.is_cancelled());
        assert_eq!(index.len(), 2);
    }

    #[tokio::test]
    async fn release_removes_the_session() {
        let index = index();
        let session = SessionId::from_u128(7);
        let connection = client(an_identity(), 1);
        let (_sender, _) = connect(&index, a_database(), connection, session).await;

        index.release_session(a_database(), connection, session);

        assert!(index.is_empty());
    }

    #[tokio::test]
    async fn superseded_connection_release_does_not_evict_its_replacement() {
        let index = index();
        let session = SessionId::from_u128(7);
        let old = client(an_identity(), 1);
        let new = client(an_identity(), 2);
        let (_old_sender, _) = connect(&index, a_database(), old, session).await;
        let (_new_sender, _) = connect(&index, a_database(), new, session).await;

        // The old connection tears down after being superseded.
        index.release_session(a_database(), old, session);

        // The replacement still holds the session.
        assert_eq!(index.len(), 1);
        let (_third, superseded) = connect(&index, a_database(), client(an_identity(), 3), session).await;
        assert_eq!(superseded.map(|old| old.connection_id), Some(new.connection_id));
    }

    /// A connection which never came to be, because `client_connected`
    /// rejected it, gives the session up again.
    #[tokio::test]
    async fn dropped_claim_releases_the_session() {
        let index = index();
        let session = SessionId::from_u128(7);

        let (claim, _) = claim(&index, a_database(), client(an_identity(), 1), session).await;
        drop(claim);

        assert!(index.is_empty());
    }

    #[tokio::test]
    async fn three_way_race_supersedes_the_most_recent_connection() {
        let index = index();
        let session = SessionId::from_u128(7);
        let (_first, _) = connect(&index, a_database(), client(an_identity(), 1), session).await;

        let (_second, superseded_by_second) = connect(&index, a_database(), client(an_identity(), 2), session).await;
        let (_third, superseded_by_third) = connect(&index, a_database(), client(an_identity(), 3), session).await;

        assert_eq!(
            superseded_by_second.map(|old| old.connection_id),
            Some(ConnectionId::from_u128(1))
        );
        assert_eq!(
            superseded_by_third.map(|old| old.connection_id),
            Some(ConnectionId::from_u128(2))
        );
    }

    /// A claim waits for the handover in flight, so it never supersedes a
    /// connection which does not exist yet and so cannot be stopped.
    #[tokio::test]
    async fn handover_serializes_concurrent_claims() {
        let index = index();
        let session = SessionId::from_u128(7);
        let first = client(an_identity(), 1);
        let second = client(an_identity(), 2);

        // The first connection claims the session but is not established yet,
        // as it would not be while its `client_connected` runs.
        let (claim, _) = claim(&index, a_database(), first, session).await;

        let torn_down = Arc::new(Mutex::new(None));
        let started = Arc::new(AtomicBool::new(false));
        let claimed = Arc::new(AtomicBool::new(false));
        let racing = tokio::spawn({
            let (index, torn_down) = (index.clone(), torn_down.clone());
            let (started, claimed) = (started.clone(), claimed.clone());
            async move {
                started.store(true, Ordering::Release);
                let claim = index
                    .claim_session(a_database(), second, session, async |old| {
                        *torn_down.lock().unwrap() = Some(old);
                    })
                    .await;
                claimed.store(true, Ordering::Release);
                claim.attach_sender(&sender(second));
            }
        });

        // The second connection cannot claim the session while the first
        // connection's handover is still in flight.
        tokio::task::yield_now().await;
        assert!(started.load(Ordering::Acquire), "the racing claim never ran");
        assert!(!claimed.load(Ordering::Acquire), "the racing claim should be waiting");
        assert!(torn_down.lock().unwrap().is_none());

        // Once the first connection is established, the second supersedes it,
        // and finds a connection which it can stop.
        let first_sender = sender(first);
        claim.attach_sender(&first_sender);
        racing.await.unwrap();

        assert_eq!(torn_down.lock().unwrap().map(|old| old.name), Some(first.name));
        assert!(first_sender.is_cancelled());
        assert_eq!(index.len(), 1);
    }
}
