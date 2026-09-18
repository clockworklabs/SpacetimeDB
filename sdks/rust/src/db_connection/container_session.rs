//! Explicit ownership of successive container-authenticated connections.

use super::{ConnectionLifecycle, DbConnectionBuilder, DbContextImpl, SpacetimeModule, WsParams};
use crate::{
    credentials::{Container, ContainerCredentialError, ContainerToken},
    Identity,
};
use futures::FutureExt;
use http::Uri;
use std::{
    panic::{resume_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};
use tokio::{
    sync::Notify,
    task::AbortHandle,
    time::{sleep_until, Instant},
};

#[cfg(test)]
mod tests;

const RETRY_MIN: Duration = Duration::from_millis(250);
const RETRY_MAX: Duration = Duration::from_secs(5);
const MIN_EXTENSION: Duration = Duration::from_secs(1);
const REFRESH_MARGIN: Duration = Duration::from_secs(10);

type Result<T> = std::result::Result<T, ContainerSessionError>;
type Configure<M> = Box<dyn FnMut(ContainerSessionInfo, DbConnectionBuilder<M>) -> DbConnectionBuilder<M> + Send>;

/// One connection generation. The target is resolved once for the owner's
/// lifetime; reconnecting cannot follow a database name to another Identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerSessionInfo {
    pub generation: u64,
    pub target: Identity,
}

/// Why a managed connection is being closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContainerSessionEndReason {
    CredentialRenewal,
    CredentialExpiry,
    Disconnected,
    Shutdown,
    Failed,
}

/// Unconfirmed reducer and procedure calls may already have committed.
/// The session does not track individual calls and never replays them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutstandingCallOutcomes {
    Unknown,
}

/// Redacted reason for a retry. No endpoint, token, or server response is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContainerSessionRetryReason {
    Credential(ContainerCredentialError),
    ExpiryNotExtended,
    Connection,
}

/// Session events contain no credentials. `Connected` means the server sent its
/// initial connection message. Subscriptions become ready separately, through
/// their normal `on_applied` callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContainerSessionEvent {
    Connecting(ContainerSessionInfo),
    Connected(ContainerSessionInfo),
    /// The outgoing channel is sealed before this event is delivered.
    Closing {
        session: ContainerSessionInfo,
        reason: ContainerSessionEndReason,
        outstanding_calls: OutstandingCallOutcomes,
    },
    /// Both native tasks have been joined before this event is delivered.
    Closed {
        session: ContainerSessionInfo,
        reason: ContainerSessionEndReason,
        outstanding_calls: OutstandingCallOutcomes,
    },
    Retrying(ContainerSessionRetryReason),
}

/// Redacted, terminal session errors. A denied credential is never retried or
/// replaced with an owner or anonymous credential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ContainerSessionError {
    #[error("Invalid container session configuration")]
    Configuration,
    #[error("Container credential error: {0}")]
    Credential(ContainerCredentialError),
    #[error("Container session server returned a different sender Identity")]
    IdentityMismatch,
    #[error("Container session connection tasks failed during shutdown")]
    Shutdown,
    #[error("Container session application callback panicked")]
    Panicked,
    #[error("Container session generation counter exhausted")]
    GenerationExhausted,
}

/// Own successive connections authenticated through a container's broker.
///
/// Pass a generated `DbConnection::builder()` with container discovery and an
/// optional server and database selection. The factory receives a fresh builder
/// for every attempt. Use it to create callbacks and recreate subscriptions in
/// `on_connect`. It must not set authentication, a target, or debug-file logging.
/// Put all callbacks in the factory, not in the initial builder.
///
/// ```ignore
/// let mut session = spacetimedb_sdk::ContainerSession::new(
///     DbConnection::builder()
///         .with_container_credentials(spacetimedb_sdk::credentials::Container::from_env()?),
///     |generation, builder| builder.on_connect(move |conn, _, _| {
///         // Register callbacks and build new subscriptions for this generation.
///         conn.subscription_builder().subscribe("SELECT * FROM jobs");
///     }),
/// )?.on_event(|event| { /* observe rotation and uncertain outstanding calls */ });
/// tokio::select! {
///     result = session.run() => result?,
///     _ = shutdown_signal => {},
/// }
/// session.shutdown_and_join().await?;
/// ```
///
/// `run` drives the connection and refreshes credentials before expiry. A token
/// that extends the lease causes a reconnect with an empty cache. There is an
/// interruption while the old connection shuts down and subscriptions rebuild.
/// The old WebSocket and parser are joined before the next WebSocket is opened.
/// Reducers and procedures are never replayed. Treat unconfirmed calls in a
/// closing generation as having unknown outcomes; use application request IDs
/// when an operation must be safely retried.
///
/// Do not drive generated connection handles independently of this owner.
/// Handles supplied to callbacks belong to that generation and become inactive
/// when it closes. A new generation gets new callbacks, subscriptions and cache.
///
/// Cancelling `run` pauses this owner, including renewal. Call `run` again to
/// resume, or call `shutdown_and_join` to finish shutdown. Cancelling the latter
/// is resumable. Dropping the owner aborts native tasks even if application code
/// retained a connection, but cannot wait for them; explicit shutdown is required
/// for completed cleanup. An application panic is propagated after task cleanup.
pub struct ContainerSession<M: SpacetimeModule> {
    container: Container,
    uri: Option<Uri>,
    database: Option<String>,
    target: Option<(Uri, Identity)>,
    params: WsParams,
    configure: Configure<M>,
    event: Option<Box<dyn FnMut(ContainerSessionEvent) + Send>>,
    current: Option<Current<M>>,
    next_token: Option<ContainerToken>,
    generation: u64,
    retry_at: Option<Instant>,
    retry_delay: Duration,
    stopped: bool,
    failure: Option<ContainerSessionError>,
}

struct Current<M: SpacetimeModule> {
    context: DbContextImpl<M>,
    info: ContainerSessionInfo,
    validity: Validity,
    refresh_at: Instant,
    signal: Arc<Signal>,
    connected_announced: bool,
    closing: Option<ContainerSessionEndReason>,
    driver_finished: bool,
    aborts: Vec<AbortHandle>,
}

#[derive(Clone, Copy)]
struct Validity {
    expiry: SystemTime,
    deadline: Instant,
}

impl Validity {
    fn from_token(token: &ContainerToken) -> Self {
        Self {
            expiry: token.expires_at(),
            deadline: token.deadline(),
        }
    }

    fn refresh_at(self) -> Instant {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        self.deadline - REFRESH_MARGIN.min(remaining / 2)
    }

    fn extended_by(self, new: Self) -> bool {
        // The broker may issue another token with the same lease expiry. A
        // fresh receipt time alone must not trigger endless reconnects.
        new.expiry
            .duration_since(self.expiry)
            .is_ok_and(|delta| delta >= MIN_EXTENSION)
            && new.deadline.saturating_duration_since(self.deadline) >= MIN_EXTENSION
    }
}

#[derive(Default)]
struct Signal {
    // 0: waiting; 1: connected; 2: unexpected sender Identity.
    state: AtomicU8,
    notify: Notify,
}

impl<M: SpacetimeModule> ContainerSession<M> {
    /// Create an owner without performing I/O. The initial builder supplies
    /// discovery, target and WebSocket options. The factory supplies callbacks.
    pub fn new(
        mut initial: DbConnectionBuilder<M>,
        configure: impl FnMut(ContainerSessionInfo, DbConnectionBuilder<M>) -> DbConnectionBuilder<M> + Send + 'static,
    ) -> Result<Self> {
        if initial.token.is_some()
            || initial.additional_logging_path.is_some()
            || initial.on_connect.is_some()
            || initial.on_disconnect.is_some()
            || initial.on_connect_error.is_some()
        {
            return Err(ContainerSessionError::Configuration);
        }
        let container = initial
            .container_credentials
            .take()
            .ok_or(ContainerSessionError::Configuration)?;
        Ok(Self {
            container,
            uri: initial.uri,
            database: initial.database_name,
            target: None,
            params: initial.params,
            configure: Box::new(configure),
            event: None,
            current: None,
            next_token: None,
            generation: 0,
            retry_at: None,
            retry_delay: RETRY_MIN,
            stopped: false,
            failure: None,
        })
    }

    /// Register a synchronous observer for redacted session events.
    pub fn on_event(mut self, event: impl FnMut(ContainerSessionEvent) + Send + 'static) -> Self {
        self.event = Some(Box::new(event));
        self
    }

    /// Drive connections and credential renewal until shutdown or a terminal
    /// error. This future owns no detached manager task and may be cancelled and
    /// resumed. Calls made through each connection retain their normal semantics.
    pub async fn run(&mut self) -> Result<()> {
        let result = AssertUnwindSafe(async {
            match self.run_loop().await {
                Ok(()) => self.failure.map_or(Ok(()), Err),
                Err(error) => {
                    self.failure.get_or_insert(error);
                    self.stopped = true;
                    self.next_token = None;
                    self.close_current(ContainerSessionEndReason::Failed);
                    self.finish_current().await?;
                    Err(self.failure.unwrap())
                }
            }
        })
        .catch_unwind()
        .await;
        match result {
            Ok(result) => result,
            Err(panic) => {
                self.stopped = true;
                self.next_token = None;
                self.failure.get_or_insert(ContainerSessionError::Panicked);
                // Retain the observer until cleanup finishes: even a callback
                // capture's destructor must not bypass task ownership.
                let observer = self.event.take();
                self.close_current(ContainerSessionEndReason::Failed);
                let _ = AssertUnwindSafe(self.finish_current()).catch_unwind().await;
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| drop(observer)));
                resume_unwind(panic)
            }
        }
    }

    /// Stop renewal, seal outgoing calls and positively join the current native
    /// connection. Safe to call again after cancellation or completed shutdown.
    pub async fn shutdown_and_join(&mut self) -> Result<()> {
        self.stopped = true;
        self.next_token = None;
        let result = AssertUnwindSafe(async {
            self.close_current(ContainerSessionEndReason::Shutdown);
            self.finish_current().await
        })
        .catch_unwind()
        .await;
        match result {
            Ok(result) => result.and_then(|()| self.failure.map_or(Ok(()), Err)),
            Err(panic) => {
                self.failure.get_or_insert(ContainerSessionError::Panicked);
                let observer = self.event.take();
                let _ = AssertUnwindSafe(self.finish_current()).catch_unwind().await;
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| drop(observer)));
                resume_unwind(panic)
            }
        }
    }

    async fn run_loop(&mut self) -> Result<()> {
        loop {
            if self.current.as_ref().is_some_and(|current| current.closing.is_some()) {
                self.finish_current().await?;
            }
            if self.stopped {
                return self.failure.map_or(Ok(()), Err);
            }
            if self.current.is_none() {
                if let Some(at) = self.retry_at {
                    sleep_until(at).await;
                    self.retry_at = None;
                }
                if self.target.is_none() {
                    // Store the concrete target before awaiting token issuance,
                    // including when a caller cancels during that request.
                    self.target = Some(
                        self.container
                            .resolve_connection_target(self.uri.as_ref(), self.database.as_deref())
                            .await
                            .map_err(ContainerSessionError::Credential)?,
                    );
                }
                let target = self.target.as_ref().unwrap().1;
                let token = match self
                    .next_token
                    .take()
                    .filter(|token| !token.remaining_lifetime().is_zero())
                {
                    Some(token) => token,
                    None => match self.container.token_for(target).await {
                        Ok(token) => token,
                        Err(error) => {
                            self.credential_retry(error)?;
                            continue;
                        }
                    },
                };
                if !self.start(token).await? {
                    continue;
                }
            }
            self.announce_connected()?;
            let current = self.current.as_ref().unwrap();
            let context = current.context.clone();
            let signal = current.signal.clone();
            let deadline = current.validity.deadline;
            let refresh_at = current.refresh_at;
            tokio::select! {
                biased;
                result = AssertUnwindSafe(context.run_async()).catch_unwind() => self.driver_finished(result)?,
                _ = signal.notify.notified() => {},
                _ = sleep_until(deadline) => self.close_current(ContainerSessionEndReason::CredentialExpiry),
                _ = sleep_until(refresh_at) => self.refresh().await?,
            }
        }
    }

    async fn start(&mut self, token: ContainerToken) -> Result<bool> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(ContainerSessionError::GenerationExhausted)?;
        let (uri, target) = self.target.as_ref().unwrap().clone();
        let info = ContainerSessionInfo {
            generation: self.generation,
            target,
        };
        let mut seed = DbConnectionBuilder::new();
        seed.params = self.params;
        let mut builder = (self.configure)(info, seed);
        if builder.uri.is_some()
            || builder.database_name.is_some()
            || builder.token.is_some()
            || builder.container_credentials.is_some()
            || builder.additional_logging_path.is_some()
        {
            return Err(ContainerSessionError::Configuration);
        }
        let signal = Arc::new(Signal::default());
        let connected_signal = signal.clone();
        let expected_sender = self.container.database_identity();
        let on_connect = builder.on_connect.take();
        builder.on_connect = Some(Box::new(move |connection, identity, token| {
            if identity != expected_sender {
                connected_signal.state.store(2, Ordering::Release);
            } else {
                if let Some(callback) = on_connect {
                    callback(connection, identity, token);
                }
                connected_signal.state.store(1, Ordering::Release);
            }
            connected_signal.notify.notify_one();
        }));
        builder.uri = Some(uri);
        builder.database_name = Some(target.to_hex().to_string());
        let validity = Validity::from_token(&token);
        self.emit(ContainerSessionEvent::Connecting(info));
        let handle = tokio::runtime::Handle::current();
        let context = match builder.build_native_with_credential(handle, Some(token)).await {
            Ok(context) => context,
            Err(_) => {
                self.retry(ContainerSessionRetryReason::Connection);
                return Ok(false);
            }
        };
        // A new context has no driver yet. Its task owner cannot be contended.
        // No suspension is allowed between construction and retaining Current.
        let aborts = context
            .inner
            .lock()
            .unwrap()
            .background_tasks
            .try_lock()
            .unwrap()
            .abort_handles();
        self.current = Some(Current {
            context,
            info,
            validity,
            refresh_at: validity.refresh_at(),
            signal,
            connected_announced: false,
            closing: None,
            driver_finished: false,
            aborts,
        });
        self.retry_at = None;
        Ok(true)
    }

    async fn refresh(&mut self) -> Result<()> {
        let current = self.current.as_ref().unwrap();
        let context = current.context.clone();
        let signal = current.signal.clone();
        let deadline = current.validity.deadline;
        let target = current.info.target;
        tokio::select! {
            biased;
            result = AssertUnwindSafe(context.run_async()).catch_unwind() => self.driver_finished(result)?,
            _ = signal.notify.notified() => {},
            _ = sleep_until(deadline) => self.close_current(ContainerSessionEndReason::CredentialExpiry),
            result = self.container.token_for(target) => {
                match result {
                    Ok(token) if self.current.as_ref().unwrap().validity.extended_by(Validity::from_token(&token)) => {
                        self.next_token = Some(token);
                        self.close_current(ContainerSessionEndReason::CredentialRenewal);
                    }
                    Ok(_) => self.retry(ContainerSessionRetryReason::ExpiryNotExtended),
                    Err(error) => self.credential_retry(error)?,
                }
            }
        }
        Ok(())
    }

    fn credential_retry(&mut self, error: ContainerCredentialError) -> Result<()> {
        match error {
            ContainerCredentialError::Unavailable
            | ContainerCredentialError::Transport
            | ContainerCredentialError::Timeout => {
                self.retry(ContainerSessionRetryReason::Credential(error));
                Ok(())
            }
            _ => Err(ContainerSessionError::Credential(error)),
        }
    }

    fn retry(&mut self, reason: ContainerSessionRetryReason) {
        let at = Instant::now() + self.retry_delay;
        self.retry_delay = (self.retry_delay * 2).min(RETRY_MAX);
        if let Some(current) = self.current.as_mut() {
            current.refresh_at = at;
        } else {
            self.retry_at = Some(at);
        }
        self.emit(ContainerSessionEvent::Retrying(reason));
    }

    fn announce_connected(&mut self) -> Result<()> {
        let current = self.current.as_mut().unwrap();
        match current.signal.state.load(Ordering::Acquire) {
            2 => return Err(ContainerSessionError::IdentityMismatch),
            1 if !current.connected_announced => {
                current.connected_announced = true;
                self.retry_delay = RETRY_MIN;
                let info = current.info;
                self.emit(ContainerSessionEvent::Connected(info));
            }
            _ => {}
        }
        Ok(())
    }

    fn driver_finished(&mut self, result: std::thread::Result<crate::Result<()>>) -> Result<()> {
        // run_async joins both native tasks before returning or unwinding.
        self.current.as_mut().unwrap().driver_finished = true;
        match result {
            Err(panic) => resume_unwind(panic),
            Ok(_) => {
                // EOF can win the select immediately after InitialConnection.
                // Check the observed sender before discarding this generation.
                self.announce_connected()?;
                self.close_current(ContainerSessionEndReason::Disconnected);
                self.retry_at = Some(Instant::now() + self.retry_delay);
                self.retry_delay = (self.retry_delay * 2).min(RETRY_MAX);
                Ok(())
            }
        }
    }

    fn close_current(&mut self, reason: ContainerSessionEndReason) {
        let Some(current) = self.current.as_mut() else {
            return;
        };
        if current.closing.is_some() {
            return;
        }
        current.closing = Some(reason);
        current.request_stop();
        let session = current.info;
        self.emit(ContainerSessionEvent::Closing {
            session,
            reason,
            outstanding_calls: OutstandingCallOutcomes::Unknown,
        });
    }

    async fn finish_current(&mut self) -> Result<()> {
        let Some(current) = self.current.as_ref() else {
            return Ok(());
        };
        let reason = current.closing.expect("only a closing connection can be joined");
        let info = current.info;
        let context = current.context.clone();
        if !current.driver_finished {
            let tasks = context
                .inner
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .background_tasks
                .clone();
            let joined = tasks.lock().await.stop_and_join().await;
            // A cancelled join is resumable through the retained NativeTasks.
            self.current.as_mut().unwrap().driver_finished = true;
            if joined.is_err() {
                self.failure.get_or_insert(ContainerSessionError::Shutdown);
                self.stopped = true;
            }
        }
        // Complete normal SDK terminal cleanup only after native tasks joined.
        // This discards queued and in-flight calls without replaying them or
        // manufacturing success, including when a retained handle exists.
        let terminal = std::panic::catch_unwind(AssertUnwindSafe(|| context.end_connection(None)));
        self.current = None;
        if let Err(panic) = terminal {
            resume_unwind(panic);
        }
        self.emit(ContainerSessionEvent::Closed {
            session: info,
            reason,
            outstanding_calls: OutstandingCallOutcomes::Unknown,
        });
        self.failure.map_or(Ok(()), Err)
    }

    fn emit(&mut self, event: ContainerSessionEvent) {
        if let Some(observer) = self.event.as_mut() {
            observer(event);
        }
    }
}

impl<M: SpacetimeModule> Current<M> {
    fn request_stop(&self) {
        // Seal calls synchronously, including handles retained by applications.
        // Closing before InitialConnection has the ordinary disconnect semantics.
        let mut inner = self.context.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner.connection_lifecycle == ConnectionLifecycle::Connecting {
            inner.connection_lifecycle = ConnectionLifecycle::Ended;
        }
        drop(inner);
        let outgoing = self
            .context
            .send_chan
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        drop(outgoing);
        for task in &self.aborts {
            task.abort();
        }
    }
}

impl<M: SpacetimeModule> Drop for ContainerSession<M> {
    fn drop(&mut self) {
        if let Some(current) = &self.current {
            current.request_stop();
        }
    }
}
