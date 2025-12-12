use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use spacetimedb_commitlog::payload::{
    txdata::{Mutations, Ops},
    Txdata,
};
use spacetimedb_data_structures::map::IntSet;
use spacetimedb_datastore::{execution_context::ReducerContext, traits::TxData};
use spacetimedb_durability::{DurableOffset, TxOffset};
use spacetimedb_primitives::TableId;
use tokio::{
    runtime,
    sync::mpsc::{channel, unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender},
};

use crate::db::persistence::Durability;

/// A request to persist a transaction or to terminate the actor.
pub struct DurabilityRequest {
    reducer_context: Option<ReducerContext>,
    tx_data: Arc<TxData>,
}

/// Represents a handle to a background task that persists transactions
/// according to the [`Durability`] policy provided.
///
/// This exists to avoid holding a transaction lock while
/// preparing the [TxData] for processing by the [Durability] layer.
pub struct DurabilityWorker {
    request_tx: UnboundedSender<DurabilityRequest>,
    requested_tx_offset: AtomicU64,
    shutdown: Sender<()>,
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
            requested_tx_offset: AtomicU64::new(0),
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
            .inspect(|()| {
                self.requested_tx_offset
                    .fetch_max(tx_data.tx_offset().unwrap_or_default(), Ordering::SeqCst);
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
    /// If [Self::request_durability] is called after [Self::shutdown], the
    /// former will panic.
    pub async fn shutdown(&self) -> anyhow::Result<TxOffset> {
        self.shutdown.send(()).await?;
        // Wait for the channel to be closed.
        self.request_tx.closed().await;
        // Load the latest tx offset and wait for it to become durable.
        let latest_tx_offset = self.requested_tx_offset.load(Ordering::SeqCst);
        let durable_offset = self.durable_tx_offset().wait_for(latest_tx_offset).await?;

        Ok(durable_offset)
    }

    /// Get a handle to the tokio runtime `self` was constructed with.
    pub fn runtime(&self) -> &tokio::runtime::Handle {
        &self.runtime
    }
}

pub struct DurabilityWorkerActor {
    request_rx: UnboundedReceiver<DurabilityRequest>,
    shutdown: Receiver<()>,
    durability: Arc<Durability>,
}

impl DurabilityWorkerActor {
    /// Processes requests to do durability.
    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;

                Some(()) = self.shutdown.recv() => {
                    self.request_rx.close();
                },

                req = self.request_rx.recv() => {
                    let Some(DurabilityRequest { reducer_context, tx_data }) = req else {
                        break;
                    };
                    Self::do_durability(&*self.durability, reducer_context, &tx_data);
                }
            }
        }
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

        let is_not_ephemeral_table = |table_id: &TableId| -> bool {
            tx_data
                .ephemeral_tables()
                .map(|etables| !etables.contains(table_id))
                .unwrap_or(true)
        };

        let inserts: Box<_> = tx_data
            .inserts()
            // Skip ephemeral tables
            .filter(|(table_id, _)| is_not_ephemeral_table(table_id))
            .map(|(table_id, rowdata)| Ops {
                table_id: *table_id,
                rowdata: rowdata.clone(),
            })
            .collect();

        let truncates: IntSet<TableId> = tx_data.truncates().collect();

        let deletes: Box<_> = tx_data
            .deletes()
            .filter(|(table_id, _)| is_not_ephemeral_table(table_id))
            .map(|(table_id, rowdata)| Ops {
                table_id: *table_id,
                rowdata: rowdata.clone(),
            })
            // filter out deletes for tables that are truncated in the same transaction.
            .filter(|ops| !truncates.contains(&ops.table_id))
            .collect();

        let truncates = truncates.into_iter().filter(is_not_ephemeral_table).collect();

        let inputs = reducer_context.map(|rcx| rcx.into());

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

    use pretty_assertions::assert_matches;
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
    }

    #[tokio::test]
    async fn shutdown_waits_until_durable() {
        let durability = Arc::new(CountingDurability::default());
        let worker = DurabilityWorker::new(durability.clone(), runtime::Handle::current());

        for i in 0..=10 {
            let mut txdata = TxData::default();
            txdata.set_tx_offset(i);
            worker.request_durability(None, &Arc::new(txdata));
        }
        assert_eq!(
            10,
            worker.requested_tx_offset.load(Ordering::Relaxed),
            "worker should have requested up to tx offset 10"
        );

        let shutdown = worker.shutdown();
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
            Poll::Ready(Ok(10)),
            "shutdown returns, reporting durable offset at 10"
        );
        assert_eq!(
            Some(10),
            *durability.appended.borrow(),
            "durability should have appended up to tx offset 10"
        );
    }
}
