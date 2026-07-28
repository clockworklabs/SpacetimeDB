pub(crate) mod durability;
pub mod error;
pub mod metrics;
pub mod persistence;
pub mod relational_db;
pub mod resource;
pub mod snapshot;
pub mod sql;
pub mod update;
pub mod util;

use std::sync::Arc;

use enum_map::EnumMap;
use spacetimedb_datastore::execution_context::WorkloadType;
use spacetimedb_datastore::locking_tx_datastore::datastore::TxMetrics;
use spacetimedb_datastore::traits::TxData;
pub use spacetimedb_lib::identity;
pub use spacetimedb_lib::Identity;
pub use spacetimedb_sats::hash;
use spacetimedb_schema::reducer_name::ReducerName;

use crate::metrics::ExecutionCounters;

/// A message that is processed by the [`spawn_tx_metrics_recorder`] actor.
/// We use a separate task to record metrics to avoid blocking transactions.
pub struct MetricsMessage {
    /// The reducer the produced these metrics.
    reducer: Option<ReducerName>,
    /// Metrics from a mutable transaction.
    metrics_for_writer: Option<TxMetrics>,
    /// Metrics from a read-only transaction.
    /// A message may have metrics for both types of transactions,
    /// because metrics for a reducer and its subscription updates are recorded together.
    metrics_for_reader: Option<TxMetrics>,
    /// The row updates for an immutable transaction.
    /// Needed for insert and delete counters.
    tx_data: Option<Arc<TxData>>,
    /// Cached metrics counters for each workload type.
    counters: Arc<EnumMap<WorkloadType, ExecutionCounters>>,
}

/// The handle used to send work to the tx metrics recorder.
pub struct MetricsRecorderQueue {
    tx: spacetimedb_runtime::sync::mpsc::UnboundedSender<MetricsMessage>,
}

impl MetricsRecorderQueue {
    pub fn send_metrics(
        &self,
        reducer: Option<ReducerName>,
        metrics_for_writer: Option<TxMetrics>,
        metrics_for_reader: Option<TxMetrics>,
        tx_data: Option<Arc<TxData>>,
        counters: Arc<EnumMap<WorkloadType, ExecutionCounters>>,
    ) {
        if let Err(err) = self.tx.send(MetricsMessage {
            reducer,
            metrics_for_writer,
            metrics_for_reader,
            tx_data,
            counters,
        }) {
            log::warn!("failed to send metrics: {err}");
        }
    }
}

fn record_metrics(
    MetricsMessage {
        reducer,
        metrics_for_writer,
        metrics_for_reader,
        tx_data,
        counters,
    }: MetricsMessage,
) {
    if let Some(tx_metrics) = metrics_for_writer {
        tx_metrics.report(
            // If row updates are present,
            // they will always belong to the writer transaction.
            tx_data.as_deref(),
            reducer.as_ref(),
            |wl| &counters[wl],
        );
    }
    if let Some(tx_metrics) = metrics_for_reader {
        tx_metrics.report(
            // If row updates are present,
            // they will never belong to the reader transaction.
            // Passing row updates here will most likely panic.
            None,
            reducer.as_ref(),
            |wl| &counters[wl],
        );
    }
}

/// The metrics recorder is a side channel that the main database thread forwards metrics to.
/// While we want to avoid unnecessary compute on the critical path, communicating with other
/// threads is not free, and for this case in particular waking a parked task is not free.
///
/// Once woken by the first message, the recorder drains a bounded batch of
/// messages that are already queued before waiting again. This batches bursts
/// of transaction metrics without adding a fixed delay to every batch.
const TX_METRICS_RECORDING_BATCH_SIZE: usize = 32;

fn process_batch<T>(
    first: T,
    rx: &mut spacetimedb_runtime::sync::mpsc::UnboundedReceiver<T>,
    mut process: impl FnMut(T),
) {
    process(first);
    for _ in 1..TX_METRICS_RECORDING_BATCH_SIZE {
        let Ok(message) = rx.try_recv() else {
            break;
        };
        process(message);
    }
}

async fn run_tx_metrics_recorder(mut rx: spacetimedb_runtime::sync::mpsc::UnboundedReceiver<MetricsMessage>) {
    while let Some(metrics) = rx.recv().await {
        process_batch(metrics, &mut rx, record_metrics);
    }
}

/// Spawns a task for recording transaction metrics.
///
/// The returned queue uniquely owns the sending side of the recorder channel.
/// Dropping it closes the channel and causes the recorder task to exit.
pub fn spawn_tx_metrics_recorder(handle: &spacetimedb_runtime::Handle) -> MetricsRecorderQueue {
    let (tx, rx) = spacetimedb_runtime::sync::mpsc::unbounded_channel();
    drop(handle.spawn(run_tx_metrics_recorder(rx)));
    MetricsRecorderQueue { tx }
}
