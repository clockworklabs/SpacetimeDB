// Creates very long lines that are not very readable.
#![allow(clippy::uninlined_format_args)]

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::Context as _;
use futures::{channel::mpsc, StreamExt as _};
use log::{info, warn};
use parking_lot::RwLock;
use prometheus::{Histogram, IntGauge};
use spacetimedb_datastore::locking_tx_datastore::{committed_state::CommittedState, datastore::Locking};
use spacetimedb_durability::TxOffset;
use spacetimedb_lib::Identity;
use spacetimedb_snapshot::{CompressionStats, SnapshotRepository};
use tokio::sync::watch;

use crate::{util::asyncify, worker_metrics::WORKER_METRICS};

pub type SnapshotDatabaseState = Arc<RwLock<CommittedState>>;

/// Whether the [SnapshotWorker] should compress historical snapshots.
#[derive(Clone, Copy, Debug)]
pub enum Compression {
    Enabled,
    Disabled,
}

impl Compression {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Represents a handle to a background task that takes snapshots of a
/// [SnapshotDatabaseState] and stores them on disk.
///
/// A snapshot can be [requested][Self::request_snapshot] and will be taken when
/// the background task gets scheduled and can acquire a read lock on the
/// database state, i.e. it happens at some point in the future.
///
/// If the worker was created with [Compression::Enabled], it will compress
/// snapshots older than the latest one. Compression errors are logged, but do
/// not prevent the creation of new snapshots.
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
    compression: Compression,
}

impl SnapshotWorker {
    /// Create a new [SnapshotWorker].
    ///
    /// The handle is only partially initialized, as it is lacking the
    /// [SnapshotDatabaseState]. This allows control code to [Self::subscribe]
    /// to future snapshots before handing off the worker to the database.
    pub fn new(snapshot_repository: Arc<SnapshotRepository>, compression: Compression) -> Self {
        let latest_snapshot = snapshot_repository.latest_snapshot().ok().flatten().unwrap_or(0);
        Self {
            snapshot_created: watch::channel(latest_snapshot).0,
            request_snapshot: OnceLock::new(),
            snapshot_repository,
            compression,
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
        let repo = self.snapshot_repository.clone();
        let database = repo.database_identity();

        let actor = SnapshotWorkerActor {
            snapshot_requests: request_rx,
            database_state: state,
            snapshot_repo: repo.clone(),
            snapshot_created: self.snapshot_created.clone(),
            metrics: SnapshotMetrics::new(database),
            compression: self.compression.is_enabled().then(|| Compressor {
                snapshot_repo: repo,
                metrics: CompressionMetrics::new(database),
                stats: <_>::default(),
            }),
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

struct SnapshotMetrics {
    snapshot_timing_total: Histogram,
    snapshot_timing_inner: Histogram,
}

impl SnapshotMetrics {
    fn new(db: Identity) -> Self {
        Self {
            snapshot_timing_total: WORKER_METRICS.snapshot_creation_time_total.with_label_values(&db),
            snapshot_timing_inner: WORKER_METRICS.snapshot_creation_time_inner.with_label_values(&db),
        }
    }
}

struct SnapshotWorkerActor {
    snapshot_requests: mpsc::UnboundedReceiver<()>,
    database_state: SnapshotDatabaseState,
    snapshot_repo: Arc<SnapshotRepository>,
    snapshot_created: watch::Sender<TxOffset>,
    metrics: SnapshotMetrics,
    compression: Option<Compressor>,
}

impl SnapshotWorkerActor {
    /// Read messages from `snapshot_requests` indefinitely.
    ///
    /// For each message, a snapshot of `database_state` is taken.
    /// The offset of each successfully created snapshot is sent to the
    /// `snapshot_created` channel.
    ///
    /// If compression is enabled, it is run after successful creation of a
    /// snapshot.
    ///
    /// The `snapshot_created` message is sent _after_ the compression pass
    /// finished (yet regardless of its success). Downstream tasks can thus
    /// expect that any locks on (valid) snapshots have been released when the
    /// message is received, unless a new snapshot request is already being
    /// processed.
    async fn run(mut self) {
        while let Some(()) = self.snapshot_requests.next().await {
            match self.take_snapshot().await {
                Ok(snapshot_offset) => {
                    if let Some(compressor) = self.compression.as_mut() {
                        compressor.compress_snapshots(snapshot_offset).await;
                    }
                    self.snapshot_created.send_replace(snapshot_offset);
                }
                Err(e) => warn!("SnapshotWorker: {e:#}"),
            }
        }
    }

    async fn take_snapshot(&self) -> anyhow::Result<TxOffset> {
        let timer = self.metrics.snapshot_timing_total.start_timer();
        let inner_timer = self.metrics.snapshot_timing_inner.clone();

        let committed_state = self.database_state.clone();
        let snapshot_repo = self.snapshot_repo.clone();

        let database_identity = self.snapshot_repo.database_identity();

        let maybe_offset = asyncify(move || {
            let _timer = inner_timer.start_timer();
            Locking::take_snapshot_internal(&committed_state, &snapshot_repo)
        })
        .await
        .with_context(|| format!("error capturing snapshot of database {}", database_identity))?;
        maybe_offset
            .map(|(offset, _path)| offset)
            .inspect(|snapshot_offset| {
                let elapsed = Duration::from_secs_f64(timer.stop_and_record());
                info!(
                    "Captured snapshot of database {} at TX offset {} in {:?}",
                    database_identity, snapshot_offset, elapsed,
                );
            })
            .with_context(|| {
                format!(
                    "refusing to take snapshot of database {} at TX offset -1",
                    database_identity
                )
            })
    }
}

struct CompressionMetrics {
    timing_total: Histogram,
    timing_inner: Histogram,
    timing_single: Histogram,
    skipped: IntGauge,
    compressed: IntGauge,
    objects_compressed: IntGauge,
    objects_hardlinked: IntGauge,
}

impl CompressionMetrics {
    fn new(db: Identity) -> Self {
        Self {
            timing_total: WORKER_METRICS.snapshot_compression_time_total.with_label_values(&db),
            timing_inner: WORKER_METRICS.snapshot_compression_time_inner.with_label_values(&db),
            timing_single: WORKER_METRICS.snapshot_compression_time_single.with_label_values(&db),
            skipped: WORKER_METRICS.snapshot_compression_skipped.with_label_values(&db),
            compressed: WORKER_METRICS.snapshot_compression_compressed.with_label_values(&db),
            objects_compressed: WORKER_METRICS
                .snapshot_compression_objects_compressed
                .with_label_values(&db),
            objects_hardlinked: WORKER_METRICS
                .snapshot_compression_objects_hardlinked
                .with_label_values(&db),
        }
    }

    fn report_and_reset(
        &self,
        CompressionStats {
            skipped,
            compression_timings,
            objects,
            // Don't reset `last_compressed`, we need it for the next run.
            last_compressed: _,
        }: &mut CompressionStats,
    ) {
        self.skipped.set(*skipped as _);
        *skipped = 0;

        self.compressed.set(compression_timings.len() as _);
        for duration in compression_timings.drain(..) {
            self.timing_single.observe(duration.as_secs_f64());
        }

        self.objects_compressed.set(objects.compressed as _);
        self.objects_hardlinked.set(objects.hardlinked as _);
        objects.reset();
    }
}

struct Compressor {
    snapshot_repo: Arc<SnapshotRepository>,
    metrics: CompressionMetrics,
    stats: Option<CompressionStats>,
}

impl Compressor {
    /// Traverse the snapshots in `self.snapshot_repository` up to and excluding
    /// `latest_snapshot` and compress all snapshots that are not yet compressed.
    ///
    /// Processes the snapshots in ascending order and stops when an error
    /// occurs.
    ///
    /// The first invocation on this [Compressor] instance will traverse all
    /// snapshots, i.e. the range `..latest_snapshot`.
    /// The latest compressed snapshot is stored internally, so subsequent
    /// invocations will visit `(last_compressed + 1)..latest_snapshot`.
    async fn compress_snapshots(&mut self, latest_snapshot: TxOffset) {
        let timer = self.metrics.timing_total.start_timer();
        let inner_timer = self.metrics.timing_inner.clone();

        let snapshot_repo = self.snapshot_repo.clone();
        let database_identity = snapshot_repo.database_identity();

        let start = self
            .stats
            .as_ref()
            .and_then(|stats| stats.last_compressed)
            // If last compressed is `Some`, exclude it from the range.
            .map(|last_compressed| last_compressed + 1)
            // Otherwise, start at zero.
            .unwrap_or_default();
        let range = start..latest_snapshot;
        let mut stats = self.stats.take().unwrap_or_default();

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
        self.stats = Some(stats);

        if let Err(e) = res {
            warn!(
                "Error compressing snapshot range {:?} of database {}: {:#}",
                range, database_identity, e
            );
        } else {
            info!(
                "Compressed snapshot range {:?} of database {} in {:?}",
                range, database_identity, elapsed
            );
        }
    }
}
