use std::{sync::Arc, time::Duration};

use futures::TryFutureExt as _;
use log::{error, info};
use spacetimedb_commitlog::payload::{
    txdata::{Mutations, Ops},
    Txdata,
};
use spacetimedb_data_structures::map::IntSet;
use spacetimedb_datastore::{execution_context::ReducerContext, traits::TxData};
use spacetimedb_durability::{DurableOffset, TxOffset};
use spacetimedb_lib::Identity;
use spacetimedb_primitives::TableId;
use tokio::{
    runtime,
    sync::{
        futures::OwnedNotified,
        mpsc::{channel, unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender},
        oneshot, Notify,
    },
    time::timeout,
};

use crate::db::persistence::Durability;

/// A request to persist a transaction or to terminate the actor.
pub struct DurabilityRequest {
    reducer_context: Option<ReducerContext>,
    tx_data: Arc<TxData>,
}

type ShutdownReply = oneshot::Sender<OwnedNotified>;

/// Represents a handle to a background task that persists transactions
/// according to the [`Durability`] policy provided.
///
/// This exists to avoid holding a transaction lock while
/// preparing the [TxData] for processing by the [Durability] layer.
pub struct DurabilityWorker {
    request_tx: UnboundedSender<DurabilityRequest>,
    shutdown: Sender<ShutdownReply>,
    durability: Arc<Durability>,
    runtime: runtime::Handle,
}

/// Those who run seem to have all the fun... 🎶
const HUNG_UP: &str = "durability actor hung up / panicked";

impl DurabilityWorker {
    /// Create a new [`DurabilityWorker`] using the given `durability` policy.
    ///
    /// Background tasks will be spawned onto to provided tokio `runtime`.
    pub fn new(durability: Arc<Durability>, runtime: runtime::Handle) -> Self {
        let (request_tx, request_rx) = unbounded_channel();
        let (shutdown_tx, shutdown_rx) = channel(1);

        let actor = DurabilityWorkerActor {
            request_rx,
            shutdown: shutdown_rx,
            durability: durability.clone(),
        };
        let _enter = runtime.enter();
        tokio::spawn(actor.run());

        Self {
            request_tx,
            shutdown: shutdown_tx,
            durability,
            runtime,
        }
    }

    /// Request that a transaction be made durable.
    /// That is, if `(tx_data, ctx)` should be appended to the commitlog, do so.
    ///
    /// Note that by this stage
    /// [`spacetimedb_datastore::locking_tx_datastore::committed_state::tx_consumes_offset`]
    /// has already decided based on the reducer and operations whether the transaction should be appended;
    /// this method is responsible only for reading its decision out of the `tx_data`
    /// and calling `durability.append_tx`.
    ///
    /// This method does not block,
    /// and sends the work to an actor that collects data and calls `durability.append_tx`.
    ///
    /// # Panics
    ///
    /// Panics if the durability worker has already closed the receive end of
    /// its queue. This may happen if
    ///
    /// - the backing [Durability] has panicked, or
    /// - [Self::shutdown] was called
    ///
    pub fn request_durability(&self, reducer_context: Option<ReducerContext>, tx_data: &Arc<TxData>) {
        self.request_tx
            .send(DurabilityRequest {
                reducer_context,
                tx_data: tx_data.clone(),
            })
            .expect(HUNG_UP);
    }

    /// Get the [`DurableOffset`] of this database.
    pub fn durable_tx_offset(&self) -> DurableOffset {
        self.durability.durable_tx_offset()
    }

    /// Shut down the worker without dropping it,
    /// flushing outstanding transaction.
    ///
    /// Closes the internal channel, then waits for the [DurableOffset] to
    /// report the offset of the most recently enqueued transaction as durable.
    ///
    /// # Panics
    ///
    /// After this method was called, calling [Self::request_durability]
    /// will panic.
    pub async fn close(&self) -> Option<TxOffset> {
        let (done_tx, done_rx) = oneshot::channel();
        // Channel errors can be ignored.
        // It just means that the actor already exited.
        let _ = self
            .shutdown
            .send(done_tx)
            .map_err(drop)
            .and_then(|()| done_rx.map_err(drop))
            .and_then(|done| async move {
                done.await;
                Ok(())
            })
            .await;
        self.durability.close().await
    }

    /// Consume `self` and run [Self::close].
    ///
    /// The `lock_file` is not dropped until the shutdown is complete (either
    /// successfully or unsuccessfully). This is to prevent the database to be
    /// re-opened for writing while there is still an active background task
    /// writing to the commitlog.
    ///
    /// The shutdown task will be spawned onto the tokio runtime provided to
    /// [Self::new]. This means that the task may still be running when this
    /// method returns.
    ///
    /// `database_identity` is used to associate log records with the database
    /// owning this durability worker.
    ///
    /// This method is used in the `Drop` impl for [crate::db::relational_db::RelationalDB].
    pub(super) fn spawn_close(self, database_identity: Identity) {
        let rt = self.runtime.clone();
        rt.spawn(async move {
            let label = format!("[{database_identity}]");
            // Apply a timeout, in case `Durability::close` doesn't terminate
            // as advertised. This is a bug, but panicking here would not
            // unwind at the call site.
            match timeout(Duration::from_secs(10), self.close()).await {
                Err(_elapsed) => {
                    error!("{label} timeout waiting for durability worker shutdown");
                }
                Ok(offset) => {
                    info!("{label} durability worker shut down at tx offset: {offset:?}");
                }
            }
        });
    }
}

pub struct DurabilityWorkerActor {
    request_rx: UnboundedReceiver<DurabilityRequest>,
    shutdown: Receiver<ShutdownReply>,
    durability: Arc<Durability>,
}

impl DurabilityWorkerActor {
    /// Processes requests to do durability.
    async fn run(mut self) {
        let done = scopeguard::guard(Arc::new(Notify::new()), |done| done.notify_waiters());
        loop {
            tokio::select! {
                // Biased towards the shutdown channel,
                // so that adding new requests is prevented promptly.
                biased;

                Some(reply) = self.shutdown.recv() => {
                    self.request_rx.close();
                    let _ = reply.send(done.clone().notified_owned());
                },

                req = self.request_rx.recv() => {
                    let Some(DurabilityRequest { reducer_context, tx_data }) = req else {
                        break;
                    };
                    Self::do_durability(&*self.durability, reducer_context, &tx_data);
                }
            }
        }

        info!("durability worker actor done");
    }

    pub fn do_durability(durability: &Durability, reducer_context: Option<ReducerContext>, tx_data: &TxData) {
        if tx_data.tx_offset().is_none() {
            let name = reducer_context.as_ref().map(|rcx| &*rcx.name);
            debug_assert!(
                !tx_data.has_rows_or_connect_disconnect(name),
                "tx_data has no rows but has connect/disconnect: `{name:?}`"
            );
            return;
        }

        let is_persistent_table = |table_id: &TableId| -> bool { !tx_data.is_ephemeral_table(table_id) };

        let inserts: Box<_> = tx_data
            .inserts()
            // Skip ephemeral tables
            .filter(|(table_id, _)| is_persistent_table(table_id))
            .map(|(table_id, rowdata)| Ops {
                table_id: *table_id,
                rowdata: rowdata.clone(),
            })
            .collect();

        let truncates: IntSet<TableId> = tx_data.truncates().collect();

        let deletes: Box<_> = tx_data
            .deletes()
            .filter(|(table_id, _)| is_persistent_table(table_id))
            .map(|(table_id, rowdata)| Ops {
                table_id: *table_id,
                rowdata: rowdata.clone(),
            })
            // filter out deletes for tables that are truncated in the same transaction.
            .filter(|ops| !truncates.contains(&ops.table_id))
            .collect();

        let truncates: Box<_> = truncates.into_iter().filter(is_persistent_table).collect();

        let inputs = reducer_context.map(|rcx| rcx.into());

        debug_assert!(
            !(inserts.is_empty() && truncates.is_empty() && deletes.is_empty() && inputs.is_none()),
            "empty transaction"
        );

        let txdata = Txdata {
            inputs,
            outputs: None,
            mutations: Some(Mutations {
                inserts,
                deletes,
                truncates,
            }),
        };

        // TODO: Should measure queuing time + actual write
        // This does not block, as per trait docs.
        durability.append_tx(txdata);
    }
}

#[cfg(test)]
mod tests {
    use std::{pin::pin, task::Poll};

    use futures::FutureExt as _;
    use pretty_assertions::assert_matches;
    use spacetimedb_sats::product;
    use tokio::sync::watch;

    use super::*;
    use crate::db::relational_db::Txdata;

    #[derive(Default)]
    struct CountingDurability {
        appended: watch::Sender<Option<TxOffset>>,
        durable: watch::Sender<Option<TxOffset>>,
    }

    impl CountingDurability {
        async fn mark_durable(&self, offset: TxOffset) {
            self.appended
                .subscribe()
                .wait_for(|x| x.is_some_and(|appended_offset| appended_offset >= offset))
                .await
                .unwrap();
            self.durable.send_modify(|durable_offset| {
                durable_offset.replace(offset);
            });
        }
    }

    impl spacetimedb_durability::Durability for CountingDurability {
        type TxData = Txdata;

        fn append_tx(&self, _tx: Self::TxData) {
            self.appended.send_modify(|offset| {
                *offset = offset.map(|x| x + 1).or(Some(0));
            });
        }

        fn durable_tx_offset(&self) -> DurableOffset {
            self.durable.subscribe().into()
        }

        fn close(&self) -> spacetimedb_durability::Close {
            let mut durable = self.durable.subscribe();
            let appended = self.appended.subscribe();
            async move {
                let durable_offset = durable
                    .wait_for(|durable| match (*durable).zip(*appended.borrow()) {
                        Some((durable_offset, appended_offset)) => durable_offset >= appended_offset,
                        None => false,
                    })
                    .await
                    .unwrap();
                *durable_offset
            }
            .boxed()
        }
    }

    #[tokio::test]
    async fn shutdown_waits_until_durable() {
        let durability = Arc::new(CountingDurability::default());
        let worker = DurabilityWorker::new(durability.clone(), runtime::Handle::current());

        for i in 0..=10 {
            let mut txdata = TxData::default();
            txdata.set_tx_offset(i);
            // Ensure the transaction is non-empty.
            txdata.set_inserts_for_table(4000.into(), "foo", [product![42u8]].into());

            worker.request_durability(None, &Arc::new(txdata));
        }

        let shutdown = worker.close();
        let mut shutdown_fut = pin!(shutdown);
        assert_matches!(
            futures::poll!(&mut shutdown_fut),
            Poll::Pending,
            "shutdown should be pending because requested > durable"
        );

        durability.mark_durable(5).await;
        assert_matches!(
            futures::poll!(&mut shutdown_fut),
            Poll::Pending,
            "shutdown should be pending because requested > durable"
        );

        durability.mark_durable(10).await;
        assert_matches!(
            futures::poll!(&mut shutdown_fut),
            Poll::Ready(Some(10)),
            "shutdown returns, reporting durable offset at 10"
        );
        assert_eq!(
            Some(10),
            *durability.appended.borrow(),
            "durability should have appended up to tx offset 10"
        );
    }
}
