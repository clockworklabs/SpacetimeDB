use std::{cmp::Reverse, collections::BinaryHeap, iter, num::NonZeroUsize, sync::Arc, time::Duration};

use futures::TryFutureExt as _;
use log::{error, info};
use prometheus::IntGauge;
use spacetimedb_commitlog::payload::{
    txdata::{Mutations, Ops},
    Txdata,
};
use spacetimedb_datastore::{execution_context::ReducerContext, traits::TxData};
use spacetimedb_durability::{DurableOffset, Transaction, TxOffset};
use spacetimedb_lib::Identity;
use thiserror::Error;
use tokio::{
    runtime,
    sync::{
        futures::OwnedNotified,
        mpsc::{self, channel, Receiver, Sender},
        oneshot, Notify,
    },
    time::timeout,
};
use tracing::{info_span, Instrument as _};

use crate::{db::persistence::Durability, worker_metrics::WORKER_METRICS};

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
///
/// The durability worker is internal to [RelationalDB], which calls
/// [DurabilityWorker::request_durability] after committing a transaction.
///
/// # Transaction ordering
///
/// The backing datastore of [RelationalDB] is responsible for creating a total
/// ordering of transactions and must uphold that [TxOffset]s are monotonically
/// increasing without gaps.
///
/// However, [RelationalDB::commit_tx] respectively [RelationalDB::commit_tx_downgrade]
/// may be called from multiple threads. Because those methods are not
/// synchronized, and release the transaction lock before requesting durability,
/// it is possible for [DurabilityRequest]s to appear slightly out-of-order on
/// the worker channel.
///
/// To mitigate this, the worker keeps a window of up to `reorder_window_size`
/// requests if out-of-order requests are detected, and flushes it to the
/// underlying durability layer once it is able to linearize the offset sequence.
///
/// Since we expect out-of-order requests to happen very rarely, this measure
/// should not negatively impact throughput in the common case, unlike holding
/// the transaction lock until request submission is complete.
///
/// Note that the commitlog rejects out-of-order commits, so if a missing offset
/// arrives outside `reorder_window_size` (or never), already committed
/// transactions may be lost (by way of the durability worker crashing).
/// Those transactions will not be confirmed, however, so this is safe.
///
/// [RelationalDB]: crate::db::relational_db::RelationalDB
pub struct DurabilityWorker {
    database: Identity,
    request_tx: Sender<DurabilityRequest>,
    shutdown: Sender<ShutdownReply>,
    durability: Arc<Durability>,
    runtime: runtime::Handle,
}

impl DurabilityWorker {
    /// Create a new [`DurabilityWorker`] using the given `durability` policy.
    ///
    /// Background tasks will be spawned onto to provided tokio `runtime`.
    pub fn new(
        database: Identity,
        durability: Arc<Durability>,
        runtime: runtime::Handle,
        next_tx_offset: TxOffset,
        reorder_window_size: NonZeroUsize,
    ) -> Self {
        let (request_tx, request_rx) = channel(4 * 4096);
        let (shutdown_tx, shutdown_rx) = channel(1);

        let actor = DurabilityWorkerActor {
            request_rx,
            shutdown: shutdown_rx,
            durability: durability.clone(),
            reorder_window: ReorderWindow::new(next_tx_offset, reorder_window_size),
            reorder_window_len: WORKER_METRICS
                .durability_worker_reorder_window_length
                .with_label_values(&database),
        };
        let _enter = runtime.enter();
        tokio::spawn(
            actor
                .run()
                .instrument(info_span!("durability_worker", database = %database)),
        );

        Self {
            database,
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
    /// This method sends the work to an actor that collects data and calls `durability.append_tx`.
    /// It blocks if the queue is at capacity.
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
        // We first try to send it without blocking.
        match self.request_tx.try_reserve() {
            Ok(permit) => {
                permit.send(DurabilityRequest {
                    reducer_context,
                    tx_data: tx_data.clone(),
                });
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                panic!("durability actor vanished database={}", self.database);
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // If the channel was full, we use the blocking version.
                let start = std::time::Instant::now();
                let send = || {
                    self.request_tx.blocking_send(DurabilityRequest {
                        reducer_context,
                        tx_data: tx_data.clone(),
                    })
                };
                if tokio::runtime::Handle::try_current().is_ok() {
                    tokio::task::block_in_place(send)
                } else {
                    send()
                }
                .unwrap_or_else(|_| panic!("durability actor vanished database={}", self.database));
                // We could cache this metric, but if we are already in the blocking code path,
                // the extra time of looking up the metric is probably negligible.
                WORKER_METRICS
                    .durability_blocking_send_duration
                    .with_label_values(&self.database)
                    .observe(start.elapsed().as_secs_f64());
            }
        }
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

#[derive(Debug, Error)]
enum ReorderError {
    #[error("reordering window exceeded")]
    SizeExceeded,
    #[error("transaction offset behind expected offset")]
    TxBehind,
}

/// A bounded collection of elements ordered by [TxOffset], backed by a [BinaryHeap].
///
/// This exists to tolerate slightly out-of-order requests.
/// See the struct docs for [DurabilityWorker] for more context.
struct ReorderWindow<T> {
    heap: BinaryHeap<Reverse<TxOrdered<T>>>,
    next_tx: TxOffset,
    max_len: NonZeroUsize,
}

impl<T> ReorderWindow<T> {
    pub fn new(next_tx: TxOffset, max_len: NonZeroUsize) -> Self {
        // We expect that requests usually arrive in order,
        // so allocate only a single element for the common case.
        let heap = BinaryHeap::with_capacity(1);
        Self { heap, next_tx, max_len }
    }

    /// Push a durability request onto the heap.
    ///
    /// # Errors
    ///
    /// The method returns an error if:
    ///
    /// - the window is full, i.e. `self.len() >= self.max_len`
    /// - the `tx_offset` of the request is smaller than the next expected offset
    ///
    pub fn push(&mut self, req: TxOrdered<T>) -> Result<(), ReorderError> {
        if self.len() >= self.max_len.get() {
            return Err(ReorderError::SizeExceeded);
        }
        if req.tx_offset < self.next_tx {
            return Err(ReorderError::TxBehind);
        }
        // We've got an out-of-order request,
        // eagerly allocate the max capacity.
        if self.len() > 0 {
            self.heap.reserve_exact(self.max_len.get());
        }
        self.heap.push(Reverse(req));

        Ok(())
    }

    /// Remove all [DurabilityRequest]s in order, until a gap in the offset
    /// sequence is detected or the heap is empty.
    pub fn drain(&mut self) -> impl Iterator<Item = T> {
        iter::from_fn(|| {
            let min_tx_offset = self.heap.peek().map(|Reverse(x)| x.tx_offset);
            if min_tx_offset.is_some_and(|tx_offset| tx_offset == self.next_tx) {
                let Reverse(TxOrdered { inner: request, .. }) = self.heap.pop().unwrap();
                self.next_tx += 1;
                Some(request)
            } else {
                None
            }
        })
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }
}

pub struct DurabilityWorkerActor {
    request_rx: mpsc::Receiver<DurabilityRequest>,
    shutdown: Receiver<ShutdownReply>,
    durability: Arc<Durability>,
    reorder_window: ReorderWindow<DurabilityRequest>,
    reorder_window_len: IntGauge,
}

impl DurabilityWorkerActor {
    /// Processes requests to do durability.
    async fn run(mut self) {
        // When this future completes or is cancelled, ensure that:
        // - shutdown waiters are notified
        // - metrics are reset
        let done = scopeguard::guard(Arc::new(Notify::new()), |done| {
            done.notify_waiters();
            self.reorder_window_len.set(0);
        });

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
                    let Some(request) = req else {
                        break;
                    };
                    match request.tx_data.tx_offset() {
                        // Drop the request if it doesn't have a tx offset.
                        None => {
                            let name = request.reducer_context.as_ref().map(|rcx| &rcx.name);
                            debug_assert!(
                                !request.tx_data.has_rows_or_connect_disconnect(name),
                                "tx_data has no rows but has connect/disconnect: `{name:?}`"
                            );
                        },
                        // Otherwise, push to the reordering window.
                        Some(tx_offset) => {
                            let request = TxOrdered { tx_offset, inner: request };
                            if let Err(e) = self.reorder_window.push(request) {
                                error!("{e}");
                                break;
                            }
                        },
                    }
                }
            }

            // Drain all requests that are properly ordered.
            self.reorder_window
                .drain()
                .for_each(|request| Self::do_durability(&*self.durability, request.reducer_context, &request.tx_data));
            self.reorder_window_len.set(self.reorder_window.len() as _);
        }

        info!("durability worker actor done");
    }

    pub fn do_durability(durability: &Durability, reducer_context: Option<ReducerContext>, tx_data: &TxData) {
        let tx_offset = tx_data
            .tx_offset()
            .expect("txs without offset should have been dropped");

        let mut inserts: Box<_> = tx_data
            .persistent_inserts()
            .map(|(table_id, rowdata)| Ops { table_id, rowdata })
            .collect();
        // What we get from `tx_data` is not necessarily sorted,
        // but the durability layer expects by-table_id sorted data.
        // Unstable sorts are valid, there will only ever be one entry per table_id.
        inserts.sort_unstable_by_key(|ops| ops.table_id);

        let mut deletes: Box<_> = tx_data
            .persistent_deletes()
            .map(|(table_id, rowdata)| Ops { table_id, rowdata })
            .collect();
        deletes.sort_unstable_by_key(|ops| ops.table_id);

        let mut truncates: Box<[_]> = tx_data.persistent_truncates().collect();
        truncates.sort_unstable_by_key(|table_id| *table_id);

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

        // This does not block, as per trait docs.
        durability.append_tx(Transaction {
            offset: tx_offset,
            txdata,
        });
    }
}

/// Wrapper to sort [DurabilityRequest]s by [TxOffset].
struct TxOrdered<T> {
    tx_offset: TxOffset,
    inner: T,
}

impl<T> PartialEq for TxOrdered<T> {
    fn eq(&self, other: &Self) -> bool {
        self.tx_offset == other.tx_offset
    }
}

impl<T> Eq for TxOrdered<T> {}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl<T> PartialOrd for TxOrdered<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.tx_offset.cmp(&other.tx_offset))
    }
}

impl<T> Ord for TxOrdered<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{pin::pin, task::Poll};

    use futures::FutureExt as _;
    use pretty_assertions::assert_matches;
    use spacetimedb_sats::product;
    use spacetimedb_schema::table_name::TableName;
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

        fn append_tx(&self, tx: Transaction<Self::TxData>) {
            self.appended.send_modify(|offset| {
                offset.replace(tx.offset);
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

    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_waits_until_durable() {
        let durability = Arc::new(CountingDurability::default());
        let worker = DurabilityWorker::new(
            Identity::ONE,
            durability.clone(),
            runtime::Handle::current(),
            0,
            NonZeroUsize::new(1).unwrap(),
        );
        for i in 0..=10 {
            let mut txdata = TxData::default();
            txdata.set_tx_offset(i);
            // Ensure the transaction is non-empty.
            txdata.set_inserts_for_table(4000.into(), &TableName::for_test("foo"), [product![42u8]].into());

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

    #[test]
    fn reorder_window_sorts_by_tx_offset() {
        let mut win = ReorderWindow::new(0, NonZeroUsize::new(5).unwrap());

        for tx_offset in (0..5).rev() {
            win.push(TxOrdered {
                tx_offset,
                inner: tx_offset,
            })
            .unwrap();
        }

        let txs = win.drain().collect::<Vec<_>>();
        assert_eq!(txs, &[0, 1, 2, 3, 4]);
    }

    #[test]
    fn reorder_window_stops_drain_at_gap() {
        let mut win = ReorderWindow::new(0, NonZeroUsize::new(5).unwrap());

        win.push(TxOrdered { tx_offset: 4, inner: 4 }).unwrap();
        assert!(win.drain().collect::<Vec<_>>().is_empty());

        for tx_offset in 0..4 {
            win.push(TxOrdered {
                tx_offset,
                inner: tx_offset,
            })
            .unwrap();
        }

        let txs = win.drain().collect::<Vec<_>>();
        assert_eq!(&txs, &[0, 1, 2, 3, 4]);
    }

    #[test]
    fn reorder_window_error_when_full() {
        let mut win = ReorderWindow::new(0, NonZeroUsize::new(1).unwrap());
        win.push(TxOrdered {
            tx_offset: 0,
            inner: (),
        })
        .unwrap();
        assert_matches!(
            win.push(TxOrdered {
                tx_offset: 1,
                inner: ()
            }),
            Err(ReorderError::SizeExceeded)
        );
    }

    #[test]
    fn reorder_window_error_on_late_request() {
        let mut win = ReorderWindow::new(1, NonZeroUsize::new(5).unwrap());
        assert_matches!(
            win.push(TxOrdered {
                tx_offset: 0,
                inner: ()
            }),
            Err(ReorderError::TxBehind)
        );
    }
}
