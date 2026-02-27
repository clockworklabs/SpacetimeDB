use std::{
    io,
    num::NonZeroUsize,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering::Relaxed},
        Arc,
    },
};

use futures::{FutureExt as _, TryFutureExt as _};
use itertools::Itertools as _;
use log::{info, trace, warn};
use scopeguard::ScopeGuard;
use spacetimedb_commitlog::{error, payload::Txdata, Commit, Commitlog, Decoder, Encode, Transaction};
use spacetimedb_fs_utils::lockfile::advisory::{LockError, LockedFile};
use spacetimedb_paths::server::ReplicaDir;
use thiserror::Error;
use tokio::{
    sync::{futures::OwnedNotified, mpsc, oneshot, watch, Notify},
    task::{spawn_blocking, AbortHandle},
};
use tracing::{instrument, Span};

use crate::{Close, Durability, DurableOffset, History, TxOffset};

pub use spacetimedb_commitlog::repo::{OnNewSegmentFn, SizeOnDisk};

/// [`Local`] configuration.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The number of elements to reserve for batching transactions.
    ///
    /// This puts an upper bound on the buffer capacity, while not preventing
    /// reallocations when the number of queued transactions exceeds it.
    ///
    /// In other words, the durability actor will attempt to receive all
    /// transactions that are currently in the queue, but shrink the buffer to
    /// `batch_capacity` if it had to make additional space during a burst.
    ///
    /// Default: 4096
    pub batch_capacity: NonZeroUsize,
    /// [`Commitlog`] configuration.
    pub commitlog: spacetimedb_commitlog::Options,
}

impl Options {
    pub const DEFAULT_BATCH_CAPACITY: NonZeroUsize = NonZeroUsize::new(4096).unwrap();
}

impl Default for Options {
    fn default() -> Self {
        Self {
            batch_capacity: Self::DEFAULT_BATCH_CAPACITY,
            commitlog: Default::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum OpenError {
    #[error("commitlog directory is locked")]
    Lock(#[from] LockError),
    #[error("failed to open commitlog")]
    Commitlog(#[from] io::Error),
}

type ShutdownReply = oneshot::Sender<OwnedNotified>;

/// [`Durability`] implementation backed by a [`Commitlog`] on local storage.
///
/// The commitlog is constrained to store the canonical [`Txdata`] payload,
/// where the generic parameter `T` is the type of the row data stored in
/// the mutations section.
///
/// `T` is left generic in order to allow bypassing the `ProductValue`
/// intermediate representation in the future.
///
/// Note, however, that instantiating `T` to a different type may require to
/// change the log format version!
pub struct Local<T> {
    /// The [`Commitlog`] this [`Durability`] and [`History`] impl wraps.
    clog: Arc<Commitlog<Txdata<T>>>,
    /// The durable transaction offset, as reported by the background
    /// [`FlushAndSyncTask`].
    durable_offset: watch::Receiver<Option<TxOffset>>,
    /// Backlog of transactions to be written to disk by the background
    /// [`PersisterTask`].
    ///
    /// Note that this is unbounded!
    queue: mpsc::UnboundedSender<Transaction<Txdata<T>>>,
    /// How many transactions are sitting in the `queue`.
    ///
    /// This is mainly for observability purposes, and can thus be updated with
    /// relaxed memory ordering.
    queue_depth: Arc<AtomicU64>,
    /// Channel to request the actor to exit.
    shutdown: mpsc::Sender<ShutdownReply>,
    /// [AbortHandle] to force cancellation of the [Actor].
    abort: AbortHandle,
}

impl<T: Encode + Send + Sync + 'static> Local<T> {
    /// Create a [`Local`] instance at the `replica_dir`.
    ///
    /// `replica_dir` must already exist.
    ///
    /// Background tasks are spawned onto the provided tokio runtime.
    ///
    /// We will send a message down the `on_new_segment` channel whenever we begin a new commitlog segment.
    /// This is used to capture a snapshot each new segment.
    pub fn open(
        replica_dir: ReplicaDir,
        rt: tokio::runtime::Handle,
        opts: Options,
        on_new_segment: Option<Arc<OnNewSegmentFn>>,
    ) -> Result<Self, OpenError> {
        info!("open local durability");

        // We could just place a lock on the commitlog directory,
        // yet for backwards-compatibility, we keep using the `db.lock` file.
        let lock = Lock::create(replica_dir.0.join("db.lock"))?;

        let clog = Arc::new(Commitlog::open(
            replica_dir.commit_log(),
            opts.commitlog,
            on_new_segment,
        )?);
        let (queue, txdata_rx) = mpsc::unbounded_channel();
        let queue_depth = Arc::new(AtomicU64::new(0));
        let (durable_tx, durable_rx) = watch::channel(clog.max_committed_offset());
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        let abort = rt
            .spawn(
                Actor {
                    clog: clog.clone(),

                    durable_offset: durable_tx,
                    queue_depth: queue_depth.clone(),

                    batch_capacity: opts.batch_capacity,

                    lock,
                }
                .run(txdata_rx, shutdown_rx),
            )
            .abort_handle();

        Ok(Self {
            clog,
            durable_offset: durable_rx,
            queue,
            shutdown: shutdown_tx,
            queue_depth,
            abort,
        })
    }

    /// Obtain a read-only copy of the durable state that implements [History].
    pub fn as_history(&self) -> impl History<TxData = Txdata<T>> {
        self.clog.clone()
    }
}

impl<T: Send + Sync + 'static> Local<T> {
    /// Inspect how many transactions added via [`Self::append_tx`] are pending
    /// to be applied to the underlying [`Commitlog`].
    pub fn queue_depth(&self) -> u64 {
        self.queue_depth.load(Relaxed)
    }

    /// Obtain an iterator over the [`Commit`]s in the underlying log.
    pub fn commits_from(&self, offset: TxOffset) -> impl Iterator<Item = Result<Commit, error::Traversal>> {
        self.clog.commits_from(offset).map_ok(Commit::from)
    }

    /// Get a list of segment offsets, sorted in ascending order.
    pub fn existing_segment_offsets(&self) -> io::Result<Vec<TxOffset>> {
        self.clog.existing_segment_offsets()
    }

    /// Compress the segments at the offsets provided, marking them as immutable.
    pub fn compress_segments(&self, offsets: &[TxOffset]) -> io::Result<()> {
        self.clog.compress_segments(offsets)
    }

    /// Get the size on disk of the underlying [`Commitlog`].
    pub fn size_on_disk(&self) -> io::Result<SizeOnDisk> {
        self.clog.size_on_disk()
    }
}

struct Actor<T> {
    clog: Arc<Commitlog<Txdata<T>>>,

    durable_offset: watch::Sender<Option<TxOffset>>,
    queue_depth: Arc<AtomicU64>,

    batch_capacity: NonZeroUsize,

    #[allow(unused)]
    lock: Lock,
}

impl<T: Encode + Send + Sync + 'static> Actor<T> {
    #[instrument(name = "durability::local::actor", skip_all)]
    async fn run(
        self,
        mut transactions_rx: mpsc::UnboundedReceiver<Transaction<Txdata<T>>>,
        mut shutdown_rx: mpsc::Receiver<oneshot::Sender<OwnedNotified>>,
    ) {
        info!("starting durability actor");

        let mut tx_buf = Vec::with_capacity(self.batch_capacity.get());
        // `flush_and_sync` when the loop exits without panicking,
        // or `flush_and_sync` inside the loop failed.
        let mut sync_on_exit = true;

        loop {
            tokio::select! {
                // Biased towards the shutdown channel,
                // so that we stop accepting new data promptly after
                // `Durability::close` was called.
                biased;

                Some(reply) = shutdown_rx.recv() => {
                    transactions_rx.close();
                    let _ = reply.send(self.lock.notified());
                },

                // Pop as many elements from the channel as possible,
                // potentially requiring the `tx_buf` to allocate additional
                // capacity.
                // We'll reclaim capacity in excess of `self.batch_size` below.
                n = transactions_rx.recv_many(&mut tx_buf, usize::MAX) => {
                    if n == 0 {
                        break;
                    }
                    self.queue_depth.fetch_sub(n as u64, Relaxed);
                    let clog = self.clog.clone();
                    tx_buf = spawn_blocking(move || -> io::Result<Vec<Transaction<Txdata<T>>>> {
                        for tx in tx_buf.drain(..) {
                            clog.commit([tx])?;
                        }
                        Ok(tx_buf)
                    })
                    .await
                    .expect("commitlog write panicked")
                    .expect("commitlog write failed");
                    if self.flush_and_sync().await.is_err() {
                        sync_on_exit = false;
                        break;
                    }
                    // Reclaim burst capacity.
                    tx_buf.shrink_to(self.batch_capacity.get());
                },
            }
        }

        if sync_on_exit {
            let _ = self.flush_and_sync().await;
        }

        info!("exiting durability actor");
    }

    #[instrument(skip_all)]
    async fn flush_and_sync(&self) -> io::Result<Option<TxOffset>> {
        // Skip if nothing changed.
        if let Some((committed, durable)) = self.clog.max_committed_offset().zip(*self.durable_offset.borrow()) {
            if committed == durable {
                return Ok(None);
            }
        }

        let clog = self.clog.clone();
        let span = Span::current();
        spawn_blocking(move || {
            let _span = span.enter();
            clog.flush_and_sync()
        })
        .await
        .expect("commitlog flush-and-sync blocking task panicked")
        .inspect_err(|e| warn!("error flushing commitlog: {e:#}"))
        .inspect(|maybe_offset| {
            if let Some(new_offset) = maybe_offset {
                trace!("synced to offset {new_offset}");
                self.durable_offset.send_modify(|val| {
                    val.replace(*new_offset);
                });
            }
        })
    }
}

struct Lock {
    file: Option<LockedFile>,
    notify_on_drop: Arc<Notify>,
}

impl Lock {
    pub fn create(path: PathBuf) -> Result<Self, LockError> {
        let file = LockedFile::lock(path).map(Some)?;
        let notify_on_drop = Arc::new(Notify::new());

        Ok(Self { file, notify_on_drop })
    }

    pub fn notified(&self) -> OwnedNotified {
        self.notify_on_drop.clone().notified_owned()
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Ensure the file lock is dropped before notifying.
        if let Some(file) = self.file.take() {
            drop(file);
        }
        self.notify_on_drop.notify_waiters();
    }
}

impl<T: Send + Sync + 'static> Durability for Local<T> {
    type TxData = Txdata<T>;

    fn append_tx(&self, tx: Transaction<Self::TxData>) {
        self.queue.send(tx).expect("durability actor crashed");
        self.queue_depth.fetch_add(1, Relaxed);
    }

    fn durable_tx_offset(&self) -> DurableOffset {
        self.durable_offset.clone().into()
    }

    fn close(&self) -> Close {
        info!("close local durability");

        let durable_offset = self.durable_tx_offset();
        let shutdown = self.shutdown.clone();
        // Abort actor if shutdown future is dropped.
        let abort = scopeguard::guard(self.abort.clone(), |actor| {
            warn!("close future dropped, aborting durability actor");
            actor.abort();
        });

        async move {
            let (done_tx, done_rx) = oneshot::channel();
            // Ignore channel errors - those just mean the actor is already gone.
            let _ = shutdown
                .send(done_tx)
                .map_err(drop)
                .and_then(|()| done_rx.map_err(drop))
                .and_then(|done| async move {
                    done.await;
                    Ok(())
                })
                .await;
            // Don't abort if we completed normally.
            let _ = ScopeGuard::into_inner(abort);

            durable_offset.last_seen()
        }
        .boxed()
    }
}

impl<T: Encode + 'static> History for Commitlog<Txdata<T>> {
    type TxData = Txdata<T>;

    fn fold_transactions_from<D>(&self, offset: TxOffset, decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        self.fold_transactions_from(offset, decoder)
    }

    fn transactions_from<'a, D>(
        &self,
        offset: TxOffset,
        decoder: &'a D,
    ) -> impl Iterator<Item = Result<Transaction<Self::TxData>, D::Error>>
    where
        D: Decoder<Record = Self::TxData>,
        D::Error: From<error::Traversal>,
        Self::TxData: 'a,
    {
        self.transactions_from(offset, decoder)
    }

    fn tx_range_hint(&self) -> (TxOffset, Option<TxOffset>) {
        let min = self.min_committed_offset().unwrap_or_default();
        let max = self.max_committed_offset();

        (min, max)
    }
}
