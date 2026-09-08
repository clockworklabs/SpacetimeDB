//! Operation-scoped access to initialized retained storage, without starting a
//! module. Only the configured durability writer is joined on close; shared
//! provider snapshot and archival services retain their existing ownership.

use super::*;
use crate::db::persistence::Persistence;
use futures::future::{BoxFuture, Shared};
use futures::FutureExt;
use spacetimedb_durability::{Close, DurableOffset, PreparedTx};
use std::panic::{resume_unwind, AssertUnwindSafe};
use std::sync::OnceLock;
use tokio::sync::Semaphore;

#[derive(Debug, thiserror::Error)]
pub(super) enum CloseFailure {
    #[error("module exit panicked; the storage writer was positively closed")]
    ModuleExit,
    #[error("storage writer close panicked; the replica remains quarantined")]
    WriterUnconfirmed,
}

#[cfg(test)]
pub(super) static FAIL_NEXT_MODULE_EXIT: parking_lot::Mutex<std::collections::BTreeSet<Identity>> =
    parking_lot::Mutex::new(std::collections::BTreeSet::new());

#[cfg(test)]
pub(super) static PANIC_CALLBACK_AT_INITIAL_SCHEDULER_START: parking_lot::Mutex<std::collections::BTreeSet<Identity>> =
    parking_lot::Mutex::new(std::collections::BTreeSet::new());

pub(super) fn writer_unconfirmed(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<CloseFailure>(),
        Some(CloseFailure::WriterUnconfirmed)
    )
}

/// Some providers hand the first close caller the writer's JoinHandle and let
/// later calls finish immediately. Share that *first* physical close instead.
/// In particular, partial RelationalDB replay may start close from Drop before
/// our owning task sees its error. That task must join the same close future.
pub(super) struct JoinedDurability {
    inner: Arc<dyn Durability<TxData = Txdata>>,
    close: OnceLock<Shared<BoxFuture<'static, Option<u64>>>>,
}

impl JoinedDurability {
    pub fn wrap(persistence: &mut Persistence) -> Arc<Self> {
        let joined = Arc::new(Self {
            inner: persistence.durability.clone(),
            close: OnceLock::new(),
        });
        persistence.durability = joined.clone();
        joined
    }

    pub async fn join(&self) -> Result<Option<u64>, CloseFailure> {
        AssertUnwindSafe(async { self.close().await })
            .catch_unwind()
            .await
            .map_err(|_| CloseFailure::WriterUnconfirmed)
    }
}

impl Durability for JoinedDurability {
    type TxData = Txdata;
    fn append_tx(&self, tx: PreparedTx<Txdata>) {
        self.inner.append_tx(tx);
    }
    fn durable_tx_offset(&self) -> DurableOffset {
        self.inner.durable_tx_offset()
    }
    fn close(&self) -> Close {
        self.close.get_or_init(|| self.inner.close().shared()).clone().boxed()
    }
}

/// Shared with ordinary initialization, so a failed normal replay cannot leave
/// its writer shutting down behind a later retained-storage operation.
pub(super) async fn open_database(
    controller: &HostController,
    database: &Database,
    replica_id: u64,
    retained_only: bool,
    tx_metrics_queue: Option<crate::db::MetricsRecorderQueue>,
) -> anyhow::Result<(
    Arc<RelationalDB>,
    relational_db::ConnectedClients,
    Option<Arc<JoinedDurability>>,
)> {
    if matches!(controller.default_config.storage, db::Storage::Memory) {
        anyhow::ensure!(!retained_only, "retained storage requires disk persistence");
        let (db, clients) = RelationalDB::open(
            database.database_identity,
            database.owner_identity,
            EmptyHistory::new(),
            None,
            tx_metrics_queue,
            controller.page_pool.clone(),
        )?;
        return Ok((Arc::new(db), clients, None));
    }

    let replica_dir = controller.data_dir.replica(replica_id);
    if retained_only {
        let commit_log = replica_dir.commit_log();
        // Fs::new can create a missing directory. Establish existing history
        // before calling either that helper or the configured provider.
        asyncify(move || {
            anyhow::ensure!(commit_log.is_dir(), "retained database history is absent");
            anyhow::ensure!(
                spacetimedb_commitlog::committed_meta(commit_log)?.is_some(),
                "retained database history is empty"
            );
            Ok::<_, anyhow::Error>(())
        })
        .await?;
    }
    let history = relational_db::local_history(&replica_dir).await?;
    let mut persistence = controller.persistence.persistence(database, replica_id).await?;
    let joined = JoinedDurability::wrap(&mut persistence);
    let identity = database.database_identity;
    let owner = database.owner_identity;
    let page_pool = controller.page_pool.clone();
    let opened = AssertUnwindSafe(asyncify(move || {
        RelationalDB::open(identity, owner, history, Some(persistence), tx_metrics_queue, page_pool)
    }))
    .catch_unwind()
    .await;
    let (db, clients) = match opened {
        Ok(Ok(opened)) => opened,
        Ok(Err(error)) => {
            joined.join().await?;
            return Err(error.into());
        }
        Err(panic) => {
            joined.join().await?;
            resume_unwind(panic);
        }
    };
    let db = Arc::new(db);
    if retained_only {
        let validation = db
            .metadata()
            .and_then(|metadata| metadata.ok_or_else(|| anyhow!("retained database is not initialized").into()));
        if let Err(error) = validation {
            joined.join().await?;
            drop(db);
            return Err(error.into());
        }
    }
    Ok((db, clients, Some(joined)))
}

impl HostController {
    /// Access retained initialized database storage without launching user code.
    ///
    /// This is a trusted host API, not an external authorization endpoint. The
    /// caller must confirm current leadership and operation authority. It must
    /// not retain or return the supplied database/module handles, spawn work
    /// that outlives the returned future, or reenter this replica's controller.
    /// A cold open keeps hosted admission closed and never runs initialization,
    /// lifecycle reducers, scheduled functions, or module compilation.
    ///
    /// Caller cancellation does not stop the owned operation. Its finite
    /// capacity permit and registry pin remain until its writer has closed.
    pub async fn with_retained_database<T, F, Fut>(
        &self,
        database: Database,
        replica_id: u64,
        operation: F,
    ) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(Arc<RelationalDB>, Option<ModuleHost>) -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
    {
        let permit = self
            .retained_capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| anyhow!("retained database operation capacity exhausted"))?;
        let controller = self.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let guard = controller
                .acquire_write_lock(replica_id)
                .await
                .map_err(|_| anyhow!("unable to lock retained database"))?;
            if let Some(host) = guard.as_ref() {
                anyhow::ensure!(
                    host.replica_ctx.database.database_identity == database.database_identity
                        && host.replica_ctx.database.owner_identity == database.owner_identity,
                    "retained database identity mismatch"
                );
                let module = host.module.borrow().clone();
                let db = host.replica_ctx.relational_db().clone();
                return operation(db, Some(module)).await;
            }
            let (db, _clients, joined) = match open_database(&controller, &database, replica_id, true, None).await {
                Ok(opened) => opened,
                Err(error) => {
                    if writer_unconfirmed(&error) {
                        guard.quarantine(Some(_permit));
                    }
                    return Err(error);
                }
            };
            let result = AssertUnwindSafe(async { operation(db.clone(), None).await })
                .catch_unwind()
                .await;
            // Keep the guard and permit while waiting for the actual first close
            // even if an inner helper or RelationalDB::Drop also requests close.
            if let Err(error) = joined.expect("retained open always uses disk persistence").join().await {
                guard.quarantine(Some(_permit));
                return Err(error.into());
            }
            drop(db);
            drop(guard);
            match result {
                Ok(result) => result,
                Err(panic) => resume_unwind(panic),
            }
        })
        .await?
    }
}

pub(super) async fn close_host(host: Host) -> Result<(), CloseFailure> {
    let module = host.module.borrow().clone();
    let info = module.info();
    let identity = info.database_identity;
    let table_names = info.module_def.tables().map(|table| table.name.deref());
    defer!(remove_database_gauges(&identity, table_names));
    let exited = AssertUnwindSafe(async {
        module.exit().await;
        #[cfg(test)]
        if FAIL_NEXT_MODULE_EXIT.lock().remove(&identity) {
            panic!("injected module exit failure");
        }
    })
    .catch_unwind()
    .await;
    let writer = AssertUnwindSafe(module.relational_db().shutdown()).catch_unwind().await;
    drop(host);
    writer.map_err(|_| CloseFailure::WriterUnconfirmed)?;
    exited.map_err(|_| CloseFailure::ModuleExit)
}

pub(super) fn capacity() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(2))
}
