use spacetimedb_commitlog::SizeOnDisk;

use super::database_logger::DatabaseLogger;
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::host::call_edge_tracker::CallEdgeTracker;
use crate::host::prepared_tx::PreparedTransactions;
use crate::host::reducer_router::ReducerCallRouter;
use crate::messages::control_db::Database;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use std::io;
use std::ops::Deref;
use std::sync::{Arc, OnceLock};
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
    /// Warmed HTTP/2 client for [`crate::host::instance_env::InstanceEnv::call_reducer_on_db`].
    ///
    /// `reqwest::Client` is internally an `Arc`, so cloning `ReplicaContext` shares the pool.
    pub call_reducer_client: reqwest::Client,
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
    /// 2PC prepared transactions registry. Shared between actor code and HTTP handlers
    /// for both participant (decision channels) and coordinator (persist waiters) roles.
    pub prepared_txs: PreparedTransactions,
    /// Called to unregister (restart) this module when a critical error occurs in an
    /// async task that can't panic on the WASM executor thread (e.g., 2PC persistence
    /// abort in Round 2).  Set once by `launch_module`; empty in tests.
    pub on_panic: Arc<OnceLock<Box<dyn Fn() + Send + Sync + 'static>>>,
    /// Distributed deadlock detection: tracks cross-database call edges.
    /// Standalone uses [`crate::host::call_edge_tracker::NoopCallEdgeTracker`].
    /// Blocking HTTP client for cross-database calls on the WASM executor thread.
    /// Built on a fresh OS thread to avoid tokio runtime conflicts.
    pub call_reducer_blocking_client: reqwest::blocking::Client,
    /// Distributed deadlock detection: tracks cross-database call edges.
    /// Standalone uses [`crate::host::call_edge_tracker::NoopCallEdgeTracker`].
    pub call_edge_tracker: Arc<dyn CallEdgeTracker>,
}

impl ReplicaContext {
    /// Build a warmed async `reqwest::Client` from `config`.
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

    /// Build a blocking `reqwest::blocking::Client` on a fresh OS thread
    /// to avoid conflicts with the tokio async runtime.
    pub fn new_call_reducer_blocking_client(config: &CallReducerOnDbConfig) -> reqwest::blocking::Client {
        let tcp_keepalive = config.tcp_keepalive;
        let pool_idle_timeout = config.pool_idle_timeout;
        let pool_max_idle_per_host = config.pool_max_idle_per_host;
        let timeout = config.request_timeout;
        std::thread::scope(|s| {
            s.spawn(move || {
                reqwest::blocking::Client::builder()
                    .tcp_keepalive(tcp_keepalive)
                    .http2_prior_knowledge()
                    .pool_idle_timeout(pool_idle_timeout)
                    .pool_max_idle_per_host(pool_max_idle_per_host)
                    .timeout(timeout)
                    .build()
                    .expect("failed to build call_reducer_on_db blocking HTTP client")
            })
            .join()
            .expect("blocking client builder thread panicked")
        })
    }
}

/// Execute a blocking reqwest request on a fresh OS thread, processing the
/// response inside that same thread.
///
/// In debug builds, reqwest 0.12 calls `wait::enter()` on every I/O operation
/// (send, bytes, text, ...). That function creates and immediately drops a mini
/// tokio runtime as a nesting-check, which panics if the calling thread is
/// already inside a tokio `block_on` context (e.g. the WASM executor thread).
///
/// By running both the send and all response reading inside a scoped OS thread
/// that has no tokio context, the assertion always passes. The closure `f`
/// receives the Response and must fully consume it (read body, extract headers,
/// etc.) before returning -- do not let the Response escape the closure.
pub fn execute_blocking_http<F, T>(
    client: &reqwest::blocking::Client,
    request: reqwest::blocking::RequestBuilder,
    f: F,
) -> std::result::Result<T, String>
where
    F: FnOnce(reqwest::blocking::Response) -> reqwest::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let client = client.clone();
    std::thread::scope(|s| {
        s.spawn(move || {
            let request = request.build().map_err(|e| e.to_string())?;
            client.execute(request).and_then(f).map_err(|e| e.to_string())
        })
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e))
    })
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
