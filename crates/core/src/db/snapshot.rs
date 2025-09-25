use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use futures::{channel::mpsc, StreamExt as _};
use parking_lot::RwLock;
use prometheus::Histogram;
use spacetimedb_datastore::locking_tx_datastore::{committed_state::CommittedState, datastore::Locking};
use spacetimedb_durability::TxOffset;
use spacetimedb_lib::Identity;
use spacetimedb_snapshot::SnapshotRepository;
use tokio::sync::watch;

use crate::{util::asyncify, worker_metrics::WORKER_METRICS};

pub type SnapshotDatabaseState = Arc<RwLock<CommittedState>>;

/// Represents a handle to a background task that takes snapshots of a
/// [SnapshotDatabaseState] and stores them on disk.
///
/// A snapshot can be [requested][Self::request_snapshot] and will be taken when
/// the background task gets scheduled and can acquire a read lock on the
/// database state, i.e. it happens at some point in the future.
///
/// Whenever a snapshot is complete, its [TxOffset] is published to a channel,
/// to which one can [subscribe][Self::subscribe].
///
/// The [SnapshotWorker] handle is freely cloneable, so ownership can be shared
/// between the database and control code.
#[derive(Clone)]
pub struct SnapshotWorker {
    snapshot_created: watch::Sender<TxOffset>,
    request_snapshot: OnceLock<mpsc::UnboundedSender<()>>,
    snapshot_repository: Arc<SnapshotRepository>,
}

impl SnapshotWorker {
    /// Create a new [SnapshotWorker].
    ///
    /// The handle is only partially initialized, as it is lacking the
    /// [SnapshotDatabaseState]. This allows control code to [Self::subscribe]
    /// to future snapshots before handing off the worker to the database.
    pub fn new(snapshot_repository: Arc<SnapshotRepository>) -> Self {
        let latest_snapshot = snapshot_repository.latest_snapshot().ok().flatten().unwrap_or(0);
        Self {
            snapshot_created: watch::channel(latest_snapshot).0,
            request_snapshot: OnceLock::new(),
            snapshot_repository,
        }
    }

    /// Finish the initialization of [Self] by passing a [SnapshotDatabaseState].
    ///
    /// This is called during construction of a [super::relational_db::RelationalDB].
    ///
    /// # Panics
    ///
    /// Panics if called after the worker was already initialized.
    pub(crate) fn start(&self, state: SnapshotDatabaseState) {
        let (request_tx, request_rx) = mpsc::unbounded();
        let metrics = ActorMetrics::new(self.snapshot_repository.database_identity());
        let actor = SnapshotWorkerActor {
            trigger: request_rx,
            committed_state: state,
            repo: self.snapshot_repository.clone(),
            notify_tx: self.snapshot_created.clone(),
            metrics,
        };
        tokio::spawn(actor.run());
        self.request_snapshot
            .set(request_tx)
            .expect("snapshot worker already initialized");
    }

    /// Get the [SnapshotRepository] this worker is operating on.
    pub fn repo(&self) -> &SnapshotRepository {
        &self.snapshot_repository
    }

    /// Request a snapshot to be taken.
    ///
    /// The snapshot will be taken at some point in the future.
    /// The request is dropped if the handle is not yet fully initialized.
    pub fn request_snapshot(&self) {
        if let Some(tx) = self.request_snapshot.get() {
            tx.unbounded_send(()).unwrap()
        }
    }

    /// Subscribe to the [TxOffset]s of snapshots created by this worker.
    ///
    /// Note that the returned [`watch::Receiver`] only stores the most recent
    /// snapshot offset, but can be turned into a [`futures::Stream`] using the
    /// `WatchStream` from the `tokio-stream` crate.
    pub fn subscribe(&self) -> watch::Receiver<TxOffset> {
        self.snapshot_created.subscribe()
    }
}

struct ActorMetrics {
    snapshot_timing_total: Histogram,
    snapshot_timing_inner: Histogram,
    compression_timing_total: Histogram,
    compression_timing_inner: Histogram,
}

impl ActorMetrics {
    fn new(db: Identity) -> Self {
        Self {
            snapshot_timing_total: WORKER_METRICS.snapshot_creation_time_total.with_label_values(&db),
            snapshot_timing_inner: WORKER_METRICS.snapshot_creation_time_inner.with_label_values(&db),
            compression_timing_total: WORKER_METRICS.snapshot_compression_time_total.with_label_values(&db),
            compression_timing_inner: WORKER_METRICS.snapshot_compression_time_inner.with_label_values(&db),
        }
    }
}

struct SnapshotWorkerActor {
    trigger: mpsc::UnboundedReceiver<()>,
    committed_state: Arc<RwLock<CommittedState>>,
    repo: Arc<SnapshotRepository>,
    notify_tx: watch::Sender<TxOffset>,
    metrics: ActorMetrics,
}

impl SnapshotWorkerActor {
    /// The snapshot loop takes a snapshot after each `trigger` message received.
    async fn run(mut self) {
        while let Some(()) = self.trigger.next().await {
            self.take_snapshot().await
        }
    }

    async fn take_snapshot(&self) {
        let timer = self.metrics.snapshot_timing_total.start_timer();
        let committed_state = self.committed_state.clone();
        let snapshot_repo = self.repo.clone();
        let res = asyncify({
            let inner_timer = self.metrics.snapshot_timing_inner.clone();
            move || {
                let _timer = inner_timer.start_timer();
                Locking::take_snapshot_internal(&committed_state, &snapshot_repo)
            }
        })
        .await;
        match res {
            Err(e) => {
                log::error!(
                    "Error capturing snapshot of database {:?}: {e:?}",
                    self.repo.database_identity()
                );
            }

            Ok(None) => {
                log::warn!(
                    "SnapshotWorker::take_snapshot: refusing to take snapshot of database {} at TX offset -1",
                    self.repo.database_identity()
                );
            }

            Ok(Some((tx_offset, _path))) => {
                let elapsed_secs = timer.stop_and_record();
                log::info!(
                    "Captured snapshot of database {:?} at TX offset {} in {:?}",
                    self.repo.database_identity(),
                    tx_offset,
                    Duration::from_secs_f64(elapsed_secs),
                );
                self.notify_tx.send_replace(tx_offset);

                let timer = self.metrics.compression_timing_total.start_timer();
                let snapshot_repo = self.repo.clone();
                asyncify({
                    let inner_timer = self.metrics.compression_timing_inner.clone();
                    move || {
                        let _timer = inner_timer.start_timer();
                        Locking::compress_older_snapshot_internal(&snapshot_repo, tx_offset)
                    }
                })
                .await;
                let elapsed_secs = timer.stop_and_record();
                log::info!(
                    "Compressed snapshots of database {} before offset {} in {:?}",
                    self.repo.database_identity(),
                    tx_offset,
                    Duration::from_secs_f64(elapsed_secs),
                );
            }
        }
    }
}
