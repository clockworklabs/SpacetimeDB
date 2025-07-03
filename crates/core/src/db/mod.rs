use std::sync::Arc;

use enum_map::EnumMap;
use tokio::sync::mpsc;

use crate::{
    db::datastore::{locking_tx_datastore::datastore::TxMetrics, traits::TxData},
    execution_context::WorkloadType,
    subscription::ExecutionCounters,
};

pub mod datastore;
pub mod db_metrics;
pub mod relational_db;
pub mod update;

/// Whether SpacetimeDB is run in memory, or persists objects and
/// a message log to disk.
#[derive(Clone, Copy)]
pub enum Storage {
    /// The object store is in memory, and no message log is kept.
    Memory,

    /// The object store is persisted to disk, and a message log is kept.
    Disk,
}

/// Internal database config parameters
#[derive(Clone, Copy)]
pub struct Config {
    /// Specifies the object storage model.
    pub storage: Storage,
    /// Specifies the page pool max size in bytes.
    pub page_pool_max_size: Option<usize>,
}

/// A message that is processed by the [`spawn_metrics_recorder`] actor.
/// We use a separate task to record metrics to avoid blocking transactions.
pub struct MetricsMessage {
    /// The reducer the produced these metrics.
    reducer: String,
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
    tx: mpsc::UnboundedSender<MetricsMessage>,
}

impl MetricsRecorderQueue {
    pub fn send_metrics(
        &self,
        reducer: String,
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

/// Spawns a task for recording transaction metrics.
/// Returns the handle for pushing metrics to the recorder.
pub fn spawn_tx_metrics_recorder() -> (MetricsRecorderQueue, tokio::task::AbortHandle) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let abort_handle = tokio::spawn(async move {
        while let Some(MetricsMessage {
            reducer,
            metrics_for_writer,
            metrics_for_reader,
            tx_data,
            counters,
        }) = rx.recv().await
        {
            if let Some(tx_metrics) = metrics_for_writer {
                tx_metrics.report(
                    // If row updates are present,
                    // they will always belong to the writer transaction.
                    tx_data.as_deref(),
                    &reducer,
                    |wl| &counters[wl],
                );
            }
            if let Some(tx_metrics) = metrics_for_reader {
                tx_metrics.report(
                    // If row updates are present,
                    // they will never belong to the reader transaction.
                    // Passing row updates here will most likely panic.
                    None,
                    &reducer,
                    |wl| &counters[wl],
                );
            }
        }
    })
    .abort_handle();
    (MetricsRecorderQueue { tx }, abort_handle)
}
