use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use futures::{channel::mpsc, StreamExt as _};
use log::{error, info, warn};
use parking_lot::RwLock;
use prometheus::{Histogram, IntGauge};
use spacetimedb_datastore::locking_tx_datastore::{committed_state::CommittedState, datastore::Locking};
use spacetimedb_durability::TxOffset;
use spacetimedb_lib::Identity;
use spacetimedb_snapshot::{CompressionStats, SnapshotRepository};
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
            snapshot_requests: request_rx,
            database_state: state,
            snapshot_repo: self.snapshot_repository.clone(),
            snapshot_created: self.snapshot_created.clone(),
            metrics,
            compression_stats: <_>::default(),
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
    compression_timing_single: Histogram,
    compression_skipped: IntGauge,
    compression_compressed: IntGauge,
    compression_objects_compressed: IntGauge,
    compression_objects_hardlinked: IntGauge,
}

impl ActorMetrics {
    fn new(db: Identity) -> Self {
        Self {
            snapshot_timing_total: WORKER_METRICS.snapshot_creation_time_total.with_label_values(&db),
            snapshot_timing_inner: WORKER_METRICS.snapshot_creation_time_inner.with_label_values(&db),
            compression_timing_total: WORKER_METRICS.snapshot_compression_time_total.with_label_values(&db),
            compression_timing_inner: WORKER_METRICS.snapshot_compression_time_inner.with_label_values(&db),
            compression_timing_single: WORKER_METRICS.snapshot_compression_time_single.with_label_values(&db),
            compression_skipped: WORKER_METRICS.snapshot_compression_skipped.with_label_values(&db),
            compression_compressed: WORKER_METRICS.snapshot_compression_compressed.with_label_values(&db),
            compression_objects_compressed: WORKER_METRICS
                .snapshot_compression_objects_compressed
                .with_label_values(&db),
            compression_objects_hardlinked: WORKER_METRICS
                .snapshot_compression_objects_hardlinked
                .with_label_values(&db),
        }
    }

    fn report_and_reset(
        &self,
        CompressionStats {
            skipped,
            compression_timings: compress_time,
            objects,
            // Don't reset `last_compressed`, we need it for the next run.
            last_compressed: _,
        }: &mut CompressionStats,
    ) {
        self.compression_skipped.set(*skipped as _);
        *skipped = 0;

        self.compression_compressed.set(compress_time.len() as _);
        for duration in compress_time.drain(..) {
            self.compression_timing_single.observe(duration.as_secs_f64());
        }

        self.compression_objects_compressed.set(objects.compressed as _);
        self.compression_objects_hardlinked.set(objects.hardlinked as _);
        objects.reset();
    }
}

struct SnapshotWorkerActor {
    snapshot_requests: mpsc::UnboundedReceiver<()>,
    database_state: SnapshotDatabaseState,
    snapshot_repo: Arc<SnapshotRepository>,
    snapshot_created: watch::Sender<TxOffset>,
    metrics: ActorMetrics,
    compression_stats: Option<CompressionStats>,
}

impl SnapshotWorkerActor {
    /// The snapshot loop takes a snapshot after each `trigger` message received.
    async fn run(mut self) {
        while let Some(()) = self.snapshot_requests.next().await {
            self.take_snapshot().await
        }
    }

    async fn take_snapshot(&mut self) {
        let timer = self.metrics.snapshot_timing_total.start_timer();
        let inner_timer = self.metrics.snapshot_timing_inner.clone();

        let committed_state = self.database_state.clone();
        let snapshot_repo = self.snapshot_repo.clone();

        let database_identity = self.snapshot_repo.database_identity();

        let res = asyncify(move || {
            let _timer = inner_timer.start_timer();
            Locking::take_snapshot_internal(&committed_state, &snapshot_repo)
        })
        .await;

        match res {
            Err(e) => error!("Error capturing snapshot of database {database_identity}: {e:#}"),
            Ok(None) => warn!("SnapshotWorker::take_snapshot: refusing to take snapshot of database {database_identity} at TX offset -1"),

            Ok(Some((tx_offset, _path))) => {
                let elapsed = Duration::from_secs_f64(timer.stop_and_record());
                info!("Captured snapshot of database {database_identity} at TX offset {tx_offset} in {elapsed:?}");
                self.snapshot_created.send_replace(tx_offset);
                self.compress_snapshot_repo(tx_offset).await
            }
        }
    }

    async fn compress_snapshot_repo(&mut self, latest_snapshot: TxOffset) {
        let timer = self.metrics.compression_timing_total.start_timer();
        let inner_timer = self.metrics.compression_timing_inner.clone();

        let database_identity = self.snapshot_repo.database_identity();

        let snapshot_repo = self.snapshot_repo.clone();
        // If we ran before, start at the last compressed snapshot,
        // otherwise inspect all snapshots.
        let last_compressed = self
            .compression_stats
            .as_ref()
            .and_then(|stats| stats.last_compressed)
            .unwrap_or_default();
        let range = (last_compressed + 1)..latest_snapshot;
        let mut stats = self.compression_stats.take().unwrap_or_default();

        let (mut stats, res) = asyncify({
            let range = range.clone();
            move || {
                let _timer = inner_timer.start_timer();
                let res = snapshot_repo.compress_snapshots(&mut stats, range);
                (stats, res)
            }
        })
        .await;
        let elapsed = Duration::from_secs_f64(timer.stop_and_record());
        self.metrics.report_and_reset(&mut stats);
        // Store stats for reuse.
        // `stats.last_compressed` is unchanged,
        // we'll use it as the range start in the next invocation.
        self.compression_stats = Some(stats);

        if let Err(e) = res {
            warn!("Error compressing snapshot range {range:?} of database {database_identity}: {e:#}");
        } else {
            info!("Compressed snapshot range {range:?} of database {database_identity} in {elapsed:?}");
        }
    }
}
