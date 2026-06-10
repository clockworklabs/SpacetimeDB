pub mod ast;
pub(crate) mod durability;
pub mod error;
pub mod metrics;
pub mod persistence;
pub mod relational_db;
pub mod rls;
pub mod snapshot;
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

/// A message that is processed by the [`spawn_metrics_recorder`] actor.
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
#[derive(Clone)]
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
        tx_metrics.report(tx_data.as_deref(), reducer.as_ref(), |wl| &counters[wl]);
    }
    if let Some(tx_metrics) = metrics_for_reader {
        tx_metrics.report(None, reducer.as_ref(), |wl| &counters[wl]);
    }
}

const TX_METRICS_RECORDING_INTERVAL: std::time::Duration = std::time::Duration::from_millis(5);

/// Spawns a task for recording transaction metrics.
/// Returns the handle for pushing metrics to the recorder.
pub fn spawn_tx_metrics_recorder(
    handle: &spacetimedb_runtime::Handle,
) -> (MetricsRecorderQueue, spacetimedb_runtime::AbortHandle) {
    let handle_clone = handle.clone();
    let (tx, mut rx) = spacetimedb_runtime::sync::mpsc::unbounded_channel();
    let abort_handle = handle
        .spawn(async move {
            loop {
                handle_clone.sleep(TX_METRICS_RECORDING_INTERVAL).await;
                while let Ok(metrics) = rx.try_recv() {
                    record_metrics(metrics);
                }
            }
        })
        .abort_handle();
    (MetricsRecorderQueue { tx }, abort_handle)
}
