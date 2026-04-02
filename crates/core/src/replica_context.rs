use spacetimedb_commitlog::SizeOnDisk;

use super::database_logger::DatabaseLogger;
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::host::global_tx::GlobalTxManager;
use crate::host::reducer_router::ReducerCallRouter;
use crate::messages::control_db::Database;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use spacetimedb_lib::{GlobalTxId, Timestamp};
use std::io;
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub type Result<T> = anyhow::Result<T>;

/// Configuration for the HTTP/2 client used to call reducers on remote databases.
///
/// Pass to [`ReplicaContext::new_call_reducer_client`] or supply directly when
/// constructing [`ReplicaContext`].
#[derive(Debug, Clone)]
pub struct CallReducerOnDbConfig {
    /// How long idle connections are held open. Default: 90 s.
    pub pool_idle_timeout: Duration,
    /// Max idle connections per host. Default: 8.
    pub pool_max_idle_per_host: usize,
    /// TCP keepalive sent to the OS. Default: 20 s.
    pub tcp_keepalive: Duration,
    /// Per-request timeout. Default: 30 s.
    pub request_timeout: Duration,
}

impl Default for CallReducerOnDbConfig {
    fn default() -> Self {
        Self {
            pool_idle_timeout: Duration::from_secs(90),
            pool_max_idle_per_host: 8,
            tcp_keepalive: Duration::from_secs(20),
            request_timeout: Duration::from_secs(30),
        }
    }
}

/// A "live" database.
#[derive(Clone)]
pub struct ReplicaContext {
    pub database: Database,
    pub replica_id: u64,
    pub logger: Arc<DatabaseLogger>,
    pub subscriptions: ModuleSubscriptions,
    /// Async HTTP/2 client for fire-and-forget coordinator/recovery tasks that run inside
    /// tokio async tasks (e.g. `recover_2pc_coordinator`, `send_ack_commit_to_coordinator`).
    ///
    /// `reqwest::Client` is internally an `Arc`, so cloning `ReplicaContext` shares the pool.
    pub call_reducer_client: reqwest::Client,
    /// Blocking HTTP client for cross-db calls made directly from the WASM executor thread.
    ///
    /// Used by [`crate::host::instance_env::InstanceEnv::call_reducer_on_db`] and the
    /// 2PC participant's `wait_for_2pc_decision` polling loop, both of which run on the
    /// `SingleCoreExecutor` std::thread and must block without yielding to tokio.
    ///
    /// `reqwest::blocking::Client` is also internally an `Arc`.
    pub call_reducer_blocking_client: reqwest::blocking::Client,
    /// Resolves the HTTP base URL of the leader node for a given database identity.
    ///
    /// - Standalone: always returns the local node URL ([`crate::host::reducer_router::LocalReducerRouter`]).
    /// - Cluster: queries the control DB to find the leader replica's node.
    pub call_reducer_router: Arc<dyn ReducerCallRouter>,
    /// `Authorization: Bearer <token>` value for outgoing cross-DB reducer calls.
    ///
    /// A single node-level token set once at startup and shared by all replicas on this node.
    /// Passed as a Bearer token so `anon_auth_middleware` on the target node accepts the request
    /// without generating a fresh ephemeral identity per call.
    ///
    /// `None` in contexts where no auth token is configured (e.g. unit tests).
    pub call_reducer_auth_token: Option<String>,
    /// Per-database nonce used when minting reducer transaction ids.
    pub tx_id_nonce: Arc<AtomicU32>,
    /// In-memory distributed transaction sessions and lock scheduling state.
    pub global_tx_manager: Arc<GlobalTxManager>,
    /// If true, 2PC skips internal durability waits used for crash-safe recovery.
    pub fake_2pc_persistence: bool,
}

impl ReplicaContext {
    /// Build a warmed async `reqwest::Client` from `config`.
    ///
    /// Uses HTTP/2 prior knowledge (h2c) for all connections.
    /// The server must be configured to accept h2c (HTTP/2 cleartext) connections.
    pub fn new_call_reducer_client(config: &CallReducerOnDbConfig) -> reqwest::Client {
        reqwest::Client::builder()
            .tcp_keepalive(config.tcp_keepalive)
            .pool_idle_timeout(config.pool_idle_timeout)
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .timeout(config.request_timeout)
            .http2_prior_knowledge()
            .build()
            .expect("failed to build call_reducer_on_db HTTP client")
    }

    /// Build a warmed blocking `reqwest::blocking::Client` from `config`.
    ///
    /// Used by the WASM executor thread and 2PC participant polling loop, which block their
    /// OS thread synchronously rather than yielding to tokio.
    ///
    /// `reqwest::blocking::Client::build()` internally creates and drops a mini tokio runtime,
    /// which panics if called from inside an async context. We build it on a fresh OS thread
    /// so it is safe to call from `async fn` at startup.
    pub fn new_call_reducer_blocking_client(config: &CallReducerOnDbConfig) -> reqwest::blocking::Client {
        let tcp_keepalive = config.tcp_keepalive;
        let pool_idle_timeout = config.pool_idle_timeout;
        let pool_max_idle_per_host = config.pool_max_idle_per_host;
        let timeout = config.request_timeout;
        std::thread::scope(|s| {
            s.spawn(move || {
                reqwest::blocking::Client::builder()
                    .tcp_keepalive(tcp_keepalive)
                    .pool_idle_timeout(pool_idle_timeout)
                    .pool_max_idle_per_host(pool_max_idle_per_host)
                    .timeout(timeout)
                    .http2_prior_knowledge()
                    .build()
                    .expect("failed to build call_reducer_on_db blocking HTTP client")
            })
            .join()
            .expect("blocking client builder thread panicked")
        })
    }
}

/// Outcome of [`execute_blocking_http_cancellable`].
pub enum HttpOutcome<T> {
    Done(reqwest::Result<T>),
    Cancelled,
}

/// Like [`execute_blocking_http`] but polls `should_cancel` every 50 ms while the HTTP
/// call is in-flight.  If `should_cancel()` returns `true` the function returns
/// [`HttpOutcome::Cancelled`] immediately; the background HTTP thread is detached and
/// completes on its own (its result is silently discarded).
///
/// All response reading must happen inside `f` — same rule as [`execute_blocking_http`].
pub fn execute_blocking_http_cancellable<F, T>(
    client: &reqwest::blocking::Client,
    request: reqwest::blocking::Request,
    should_cancel: impl Fn() -> bool,
    f: F,
) -> HttpOutcome<T>
where
    F: FnOnce(reqwest::blocking::Response) -> reqwest::Result<T> + Send + 'static,
    T: Send + 'static,
{
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel::<reqwest::Result<T>>();
    let client = client.clone();
    let handle = std::thread::spawn(move || {
        let result = client.execute(request).and_then(f);
        let _ = tx.send(result);
    });
    let result = loop {
        match rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(result) => break Some(result),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if should_cancel() {
                    // Drop handle — thread is detached and its result discarded.
                    return HttpOutcome::Cancelled;
                }
            }
            // Sender dropped without sending → thread panicked.
            Err(mpsc::RecvTimeoutError::Disconnected) => break None,
        }
    };
    match result {
        Some(r) => HttpOutcome::Done(r),
        None => std::panic::resume_unwind(handle.join().unwrap_err()),
    }
}

/// Execute a blocking reqwest request on a fresh OS thread, processing the response inside
/// that same thread.
///
/// In debug builds, `reqwest 0.12` calls `wait::enter()` on every I/O operation
/// (`send`, `bytes`, `text`, …). That function creates and immediately drops a mini
/// tokio runtime as a nesting-check, which panics if the calling thread is already
/// inside a tokio `block_on` context (e.g. the `SingleCoreExecutor` WASM thread).
///
/// By running both the send **and** all response reading inside a scoped OS thread that
/// has no tokio context, the assertion always passes.  The closure `f` receives the
/// `Response` and must fully consume it (read body, extract headers, etc.) before
/// returning — do not let the `Response` escape the closure.
pub fn execute_blocking_http<F, T>(
    client: &reqwest::blocking::Client,
    request: reqwest::blocking::Request,
    f: F,
) -> reqwest::Result<T>
where
    F: FnOnce(reqwest::blocking::Response) -> reqwest::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let client = client.clone();
    std::thread::scope(|s| {
        s.spawn(move || client.execute(request).and_then(f))
            .join()
            .unwrap_or_else(|e| std::panic::resume_unwind(e))
    })
}

impl ReplicaContext {
    pub fn mint_global_tx_id(&self, start_ts: Timestamp) -> GlobalTxId {
        let nonce = self.tx_id_nonce.fetch_add(1, Ordering::Relaxed);
        GlobalTxId::new(start_ts, self.database.database_identity, nonce, 0)
    }
}

impl ReplicaContext {
    /// The number of bytes on disk occupied by the database's durability layer.
    ///
    /// An in-memory database will return `Ok(0)`.
    pub fn durability_size_on_disk(&self) -> io::Result<SizeOnDisk> {
        self.relational_db().size_on_disk()
    }

    /// The size of the log file.
    pub fn log_file_size(&self) -> std::result::Result<u64, DBError> {
        Ok(self.logger.size()?)
    }

    /// Obtain an array which can be summed to obtain the total disk usage.
    ///
    /// Some sources of size-on-disk may error, in which case the corresponding array element will be None.
    pub fn total_disk_usage(&self) -> TotalDiskUsage {
        TotalDiskUsage {
            durability: self
                .durability_size_on_disk()
                .inspect_err(|e| {
                    log::error!(
                        "database={} replica={}: failed to obtain durability size on disk: {:#}",
                        self.database.database_identity,
                        self.replica_id,
                        e
                    );
                })
                .ok(),
            logs: self
                .log_file_size()
                .inspect_err(|e| {
                    log::error!(
                        "database={} replica={}: failed to obtain log file size: {:#}",
                        self.database.database_identity,
                        self.replica_id,
                        e
                    );
                })
                .ok(),
        }
    }

    /// The size in bytes of all of the in-memory data of the database.
    pub fn mem_usage(&self) -> usize {
        self.relational_db().size_in_memory()
    }

    /// Update data size stats.
    pub fn update_gauges(&self) {
        self.relational_db().update_data_size_metrics();
        self.subscriptions.update_gauges();
    }

    /// Returns a reference to the relational database.
    pub fn relational_db(&self) -> &Arc<RelationalDB> {
        self.subscriptions.relational_db()
    }
}

impl Deref for ReplicaContext {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

#[derive(Copy, Clone, Default)]
pub struct TotalDiskUsage {
    pub durability: Option<SizeOnDisk>,
    pub logs: Option<u64>,
}

impl TotalDiskUsage {
    /// Returns self, but if any of the sources are None then we take it from fallback
    pub fn or(self, fallback: TotalDiskUsage) -> Self {
        Self {
            durability: self.durability.or(fallback.durability),
            logs: self.logs.or(fallback.logs),
        }
    }
}
