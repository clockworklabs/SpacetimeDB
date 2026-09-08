//! Shared physical writer completion for ordinary module initialization and
//! shutdown. Provider snapshot and archival services keep their own ownership.

use super::*;
use crate::db::persistence::Persistence;
use futures::future::{BoxFuture, Shared};
use futures::FutureExt;
use spacetimedb_durability::{Close, DurableOffset, PreparedTx};
use std::panic::{resume_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

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

/// A failed replay joins the same physical writer close before another normal
/// initialization can acquire the replica's canonical registry cell.
pub(super) async fn open_database(
    controller: &HostController,
    database: &Database,
    replica_id: u64,
    tx_metrics_queue: Option<crate::db::MetricsRecorderQueue>,
) -> anyhow::Result<(
    Arc<RelationalDB>,
    relational_db::ConnectedClients,
    Option<Arc<JoinedDurability>>,
)> {
    if matches!(controller.default_config.storage, db::Storage::Memory) {
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
    Ok((db, clients, Some(joined)))
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
