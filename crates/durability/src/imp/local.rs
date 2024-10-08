use std::{
    io,
    num::NonZeroU16,
    panic,
    path::PathBuf,
    sync::{
        atomic::{
            AtomicI64, AtomicU64,
            Ordering::{Acquire, Relaxed, Release},
        },
        Arc,
    },
    time::Duration,
};

use anyhow::Context as _;
use itertools::Itertools as _;
use log::{info, trace, warn};
use spacetimedb_commitlog::{error, payload::Txdata, Commit, Commitlog, Decoder, Encode, Transaction};
use tokio::{
    sync::mpsc,
    task::{spawn_blocking, AbortHandle, JoinHandle},
    time::{interval, MissedTickBehavior},
};
use tracing::instrument;

use crate::{Durability, History, TxOffset};

/// [`Local`] configuration.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Periodically flush and sync the log this often.
    ///
    /// Default: 500ms
    pub sync_interval: Duration,
    /// [`Commitlog`] configuration.
    pub commitlog: spacetimedb_commitlog::Options,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            sync_interval: Duration::from_millis(500),
            commitlog: Default::default(),
        }
    }
}

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
    ///
    /// A negative number indicates that we haven't flushed yet, or that the
    /// number overflowed. In either case, appending new transactions shall panic.
    ///
    /// The offset will be used by the datastore to squash durable transactions
    /// into the committed state, thereby making them visible to durable-only
    /// readers.
    ///
    /// We don't want to hang on to those transactions longer than needed, so
    /// acquire / release or stronger should be used to prevent stale reads.
    durable_offset: Arc<AtomicI64>,
    /// Backlog of transactions to be written to disk by the background
    /// [`PersisterTask`].
    ///
    /// Note that this is unbounded!
    queue: mpsc::UnboundedSender<Txdata<T>>,
    /// How many transactions are sitting in the `queue`.
    ///
    /// This is mainly for observability purposes, and can thus be updated with
    /// relaxed memory ordering.
    queue_depth: Arc<AtomicU64>,
    /// Handle to the [`PersisterTask`], allowing to drain the `queue` when
    /// explicitly dropped via [`Self::close`].
    persister_task: JoinHandle<()>,
}

impl<T: Encode + Send + Sync + 'static> Local<T> {
    /// Create a [`Local`] instance at the `root` directory.
    ///
    /// The `root` directory must already exist.
    ///
    /// Background tasks are spawned onto the provided tokio runtime.
    pub fn open(root: impl Into<PathBuf>, rt: tokio::runtime::Handle, opts: Options) -> io::Result<Self> {
        info!("open local durability");

        let clog = Arc::new(Commitlog::open(root, opts.commitlog)?);
        let (queue, rx) = mpsc::unbounded_channel();
        let queue_depth = Arc::new(AtomicU64::new(0));
        let offset = {
            let offset = clog.max_committed_offset().map(|x| x as i64).unwrap_or(-1);
            Arc::new(AtomicI64::new(offset))
        };

        let persister_task = rt.spawn(
            PersisterTask {
                clog: clog.clone(),
                rx,
                queue_depth: queue_depth.clone(),
                max_records_in_commit: opts.commitlog.max_records_in_commit,
            }
            .run(),
        );
        rt.spawn(
            FlushAndSyncTask {
                clog: clog.clone(),
                period: opts.sync_interval,
                offset: offset.clone(),
                abort: persister_task.abort_handle(),
            }
            .run(),
        );

        Ok(Self {
            clog,
            durable_offset: offset,
            queue,
            queue_depth,
            persister_task,
        })
    }

    /// Inspect how many transactions added via [`Self::append_tx`] are pending
    /// to be applied to the underlying [`Commitlog`].
    pub fn queue_depth(&self) -> u64 {
        self.queue_depth.load(Relaxed)
    }

    /// Obtain an iterator over the [`Commit`]s in the underlying log.
    pub fn commits_from(&self, offset: TxOffset) -> impl Iterator<Item = Result<Commit, error::Traversal>> {
        self.clog.commits_from(offset).map_ok(Commit::from)
    }

    /// Apply all outstanding transactions to the [`Commitlog`] and flush it
    /// to disk.
    ///
    /// Returns the durable [`TxOffset`], if any.
    pub async fn close(self) -> anyhow::Result<Option<TxOffset>> {
        info!("close local durability");

        drop(self.queue);
        if let Err(e) = self.persister_task.await {
            if e.is_panic() {
                return Err(e).context("persister task panicked");
            }
        }

        spawn_blocking(move || self.clog.flush_and_sync())
            .await?
            .context("failed to sync commitlog")
    }

    /// Get the size on disk of the underlying [`Commitlog`].
    pub fn size_on_disk(&self) -> io::Result<u64> {
        self.clog.size_on_disk()
    }
}

struct PersisterTask<T> {
    clog: Arc<Commitlog<Txdata<T>>>,
    rx: mpsc::UnboundedReceiver<Txdata<T>>,
    queue_depth: Arc<AtomicU64>,
    max_records_in_commit: NonZeroU16,
}

impl<T: Encode + Send + Sync + 'static> PersisterTask<T> {
    #[instrument(name = "durability::local::persister_task", skip_all)]
    async fn run(mut self) {
        info!("starting persister task");

        while let Some(txdata) = self.rx.recv().await {
            self.queue_depth.fetch_sub(1, Relaxed);
            trace!("received txdata");

            // If we are writing one commit per tx, trying to buffer is
            // fairly pointless. Immediately flush instead.
            //
            // Otherwise, try `Commitlog::append` as a fast-path which doesn't
            // require `spawn_blocking`.
            if self.max_records_in_commit.get() == 1 {
                self.flush_append(txdata, true).await;
            } else if let Err(retry) = self.clog.append(txdata) {
                self.flush_append(retry, false).await
            }

            trace!("appended txdata");
        }

        info!("exiting persister task");
    }

    #[instrument(skip_all)]
    async fn flush_append(&self, txdata: Txdata<T>, flush_after: bool) {
        let clog = self.clog.clone();
        let task = spawn_blocking(move || {
            let mut retry = Some(txdata);
            while let Some(txdata) = retry.take() {
                if let Err(error::Append { txdata, source }) = clog.append_maybe_flush(txdata) {
                    flush_error(source);
                    retry = Some(txdata);
                }
            }

            if flush_after {
                clog.flush().map(drop).unwrap_or_else(flush_error);
            }

            trace!("flush-append succeeded");
        })
        .await;
        if let Err(e) = task {
            // Resume panic on the spawned task,
            // which will drop the channel receiver,
            // which will cause `append_tx` to panic.
            if e.is_panic() {
                panic::resume_unwind(e.into_panic())
            }
        }
    }
}

/// Handle an error flushing the commitlog.
///
/// Panics if the error indicates that the log may be permanently unwritable.
#[inline]
fn flush_error(e: io::Error) {
    warn!("error flushing commitlog: {e:?}");
    if e.kind() == io::ErrorKind::AlreadyExists {
        panic!("commitlog unwritable!");
    }
}

struct FlushAndSyncTask<T> {
    clog: Arc<Commitlog<Txdata<T>>>,
    period: Duration,
    offset: Arc<AtomicI64>,
    /// Handle to abort the [`PersisterTask`] if fsync panics.
    abort: AbortHandle,
}

impl<T: Send + Sync + 'static> FlushAndSyncTask<T> {
    #[instrument(name = "durability::local::flush_and_sync_task", skip_all)]
    async fn run(self) {
        info!("starting syncer task");

        let mut interval = interval(self.period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            // Skip if nothing changed.
            if let Some(committed) = self.clog.max_committed_offset() {
                let durable = self.offset.load(Acquire);
                if durable.is_positive() && committed == durable as _ {
                    continue;
                }
            }

            let clog = self.clog.clone();
            let task = spawn_blocking(move || clog.flush_and_sync()).await;
            match task {
                Err(e) => {
                    if e.is_panic() {
                        self.abort.abort();
                        panic::resume_unwind(e.into_panic())
                    }
                    break;
                }
                Ok(Err(e)) => {
                    warn!("flush failed: {e}");
                }
                Ok(Ok(Some(new_offset))) => {
                    trace!("synced to offset {new_offset}");
                    // NOTE: Overflow will make `durable_tx_offset` return `None`
                    self.offset.store(new_offset as i64, Release);
                }
                // No data to flush.
                Ok(Ok(None)) => {}
            }
        }

        info!("exiting syncer task");
    }
}

impl<T: Send + Sync + 'static> Durability for Local<T> {
    type TxData = Txdata<T>;

    fn append_tx(&self, tx: Self::TxData) {
        self.queue.send(tx).expect("commitlog persister task vanished");
        self.queue_depth.fetch_add(1, Relaxed);
    }

    fn durable_tx_offset(&self) -> Option<TxOffset> {
        let offset = self.durable_offset.load(Acquire);
        (offset > -1).then_some(offset as u64)
    }
}

impl<T: Encode + 'static> History for Local<T> {
    type TxData = Txdata<T>;

    fn fold_transactions_from<D>(&self, offset: TxOffset, decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        self.clog.fold_transactions_from(offset, decoder)
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
        self.clog.transactions_from(offset, decoder)
    }

    fn max_tx_offset(&self) -> Option<TxOffset> {
        self.clog.max_committed_offset()
    }
}
