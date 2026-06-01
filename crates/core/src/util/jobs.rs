use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, Weak};

use core_affinity::CoreId;
use futures::future::LocalBoxFuture;
use futures::FutureExt;
use indexmap::IndexMap;
use smallvec::SmallVec;
use spacetimedb_data_structures::map::HashMap;
use tokio::runtime;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::Instrument;

use crate::util::thread_scheduling::apply_compute_thread_hint;

/// A handle to a pool of Tokio executors for running database WASM code on.
///
/// Each database has a [`SingleCoreExecutor`],
/// a handle to a single OS thread with a Tokio LocalSet which is pinned to a specific CPU core.
/// In multi-tenant environments, multiple databases' [`SingleCoreExecutor`]s may be handles on the same runtime/core,
/// and a [`SingleCoreExecutor`] may occasionally be migrated to a different runtime/core to balance load.
///
/// Construct a `JobCores` via [`Self::from_pinned_cores`] or [`Self::without_pinned_cores`].
/// A `JobCores` constructed without core pinning, including `from_pinned_cores` on an empty set,
/// will spawn threads that are not pinned to any cores.
///
/// This handle is cheaply cloneable, but at least one handle must be kept alive.
/// If all instances of it are dropped, load-balancing will no longer occur when
/// threads exit or new threads are spawned.
#[derive(Clone)]
pub struct JobCores {
    inner: JobCoresInner,
}

#[derive(Clone)]
enum JobCoresInner {
    PinnedCores(Arc<Mutex<PinnedCoresExecutorManager>>),
    NoPinning,
}

struct PinnedCoresExecutorManager {
    /// Channels to request that a [`SingleCoreExecutor`] move to a different core.
    ///
    /// The [`CoreId`] that an executor is pinned to is used as an index into
    /// `self.cores` to make load-balancing decisions when freeing a database
    /// executor in [`Self::deallocate`].
    database_executor_move: HashMap<SingleCoreExecutorId, watch::Sender<CoreId>>,
    cores: IndexMap<CoreId, CoreInfo>,
    /// An index into `cores` of the next core to put a new job onto.
    ///
    /// This acts as a partition point in `cores`; all cores in `..index` have
    /// one fewer job on them than the cores in `index..`.
    next_core: usize,
    next_id: SingleCoreExecutorId,
}

/// Remembers the [`SingleCoreExecutorId`]s for all databases sharing that executor.
#[derive(Default)]
struct CoreInfo {
    jobs: SmallVec<[SingleCoreExecutorId; 4]>,
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct SingleCoreExecutorId(usize);

impl JobCores {
    /// Get an [`AllocatedCore`] for a job thread.
    pub fn take(&self) -> AllocatedJobCore {
        match &self.inner {
            JobCoresInner::NoPinning => AllocatedJobCore::default(),
            JobCoresInner::PinnedCores(manager) => {
                let manager_weak = Arc::downgrade(manager);
                let (database_executor_id, pinner) = manager.lock().unwrap().allocate();
                let guard = LoadBalanceOnDropGuard {
                    inner: Some((manager_weak, database_executor_id)),
                };
                AllocatedJobCore { guard, pinner }
            }
        }
    }

    /// Construct a [`JobCores`] which runs one Tokio runtime on each of the `cores`,
    /// and pins each database to a particular runtime/core.
    ///
    /// If `cores` is empty, this falls back to [`Self::without_pinned_cores`]
    /// and runs all databases in the `global_runtime`.
    pub fn from_pinned_cores(cores: impl IntoIterator<Item = CoreId>) -> Self {
        let cores: IndexMap<_, _> = cores.into_iter().map(|id| (id, CoreInfo::default())).collect();
        let inner = if cfg!(not(feature = "core-pinning")) || cores.is_empty() {
            JobCoresInner::NoPinning
        } else {
            JobCoresInner::PinnedCores(Arc::new(Mutex::new(PinnedCoresExecutorManager {
                database_executor_move: HashMap::default(),
                cores,
                next_core: 0,
                next_id: SingleCoreExecutorId(0),
            })))
        };

        Self { inner }
    }

    /// Construct a [`JobCores`] which does not perform any core pinning,
    /// and just runs all database jobs in `global_runtime`.
    ///
    /// This will be used in deployments where there aren't enough available CPU cores
    /// to reserve specific cores for database WASM execution.
    pub const fn without_pinned_cores() -> Self {
        Self {
            inner: JobCoresInner::NoPinning,
        }
    }
}

impl PinnedCoresExecutorManager {
    /// Get a core for running database operations on,
    /// and store state in `self` necessary to move that database to a new core
    /// for load-balancing purposes.
    ///
    /// The returned [`SingleCoreExecutorId`] is an index into internal data structures in `self` (namely, `self.cores`)
    /// which should be passed to [`Self::deallocate`] when the database is no longer using this executor.
    /// This is done automatically by [`LoadBalanceOnDropGuard`].
    ///
    /// The returned [`CorePinner`] stores the [`CoreId`] on which the database
    /// should run its compute-intensive jobs. This may occasionally be
    /// replaced to balance databases among available cores, so databases should
    /// either spawn [`CorePinner::run`] as a thread-local async task, or call
    /// [`CorePinner::pin_now`] frequently.
    fn allocate(&mut self) -> (SingleCoreExecutorId, CorePinner) {
        // Determine the next job ID.
        let database_executor_id = self.next_id;
        self.next_id.0 += 1;

        // Put the job ID into the next core.
        let core_id = {
            let (&core_id, core_info) = self
                .cores
                .get_index_mut(self.next_core)
                .expect("`self.next_core < self.cores.len()`");
            core_info.jobs.push(database_executor_id);
            core_id
        };
        // Move the next core one ahead, wrapping around the number of cores we have.
        self.next_core = (self.next_core + 1) % self.cores.len();

        // Record channels and details for moving a job to a different core.
        let (move_core_tx, move_core_rx) = watch::channel(core_id);
        self.database_executor_move.insert(database_executor_id, move_core_tx);

        let core_pinner = CorePinner {
            move_core_rx: Some(move_core_rx),
        };
        (database_executor_id, core_pinner)
    }

    /// Mark the executor at `id` as no longer in use, free internal state which tracks it,
    /// and move other executors to different cores as necessary to maintain a balanced distribution.
    ///
    /// Called by [`LoadBalanceOnDropGuard`] when a [`SingleCoreExecutor`] is no longer in use.
    fn deallocate(&mut self, id: SingleCoreExecutorId) {
        // Determine the `CoreId` that will now have one less job.
        // The `id`s came from `self.allocate()`,
        // so there must be a `database_executor_move` for it.
        let freed_core_id = *self
            .database_executor_move
            .remove(&id)
            .expect("there should be a `database_executor_move` for `id`")
            .borrow();

        let core_index = self.cores.get_index_of(&freed_core_id).unwrap();

        // This core is now less busy than it should be - bump `next_core` back
        // by 1 and steal a thread from the core there.
        //
        // This wraps around in the 0 case, so the partition point is simply
        // moved to the end of the ring buffer.

        let steal_from_index = self.next_core.checked_sub(1).unwrap_or(self.cores.len() - 1);

        // If this core was already at `next_core - 1`, we don't need to steal from anywhere.
        let (core_info, steal_from) = match self.cores.get_disjoint_indices_mut([core_index, steal_from_index]) {
            Ok([(_, core), (_, steal_from)]) => (core, Some(steal_from)),
            Err(_) => (&mut self.cores[core_index], None),
        };

        let pos = core_info.jobs.iter().position(|x| *x == id).unwrap();
        // Swap remove because we don't care about ordering within `core_info.jobs`
        core_info.jobs.swap_remove(pos);

        if let Some(steal_from) = steal_from {
            // This unwrap will never fail, since cores below `next_core` always have
            // at least 1 thread on them. Edge case: if `next_core` is 0, `steal_from`
            // would wrap around to the end - but when `next_core` is 0, every core has
            // the same number of threads; so, if the last core is empty, all the cores
            // would be empty, but we know that's impossible because we're deallocating
            // a thread right now.
            let stolen = steal_from.jobs.pop().unwrap();
            // the way we pop and push here means that older job threads will be less
            // likely to be repinned, while younger ones are liable to bounce around.
            // Our use of `swap_remove` above makes this not entirely predictable, however.
            core_info.jobs.push(stolen);
            let migrate_tx = &self.database_executor_move[&stolen];
            migrate_tx.send_replace(freed_core_id);
        }

        self.next_core = steal_from_index;
    }
}

/// Returned from [`JobCores::take`]; represents a job thread allocated to a
/// specific core.
///
/// The `guard` should be dropped when the job thread is no longer running, and
/// the `pinner` should be ran on the job thread.
#[derive(Default)]
pub struct AllocatedJobCore {
    pub guard: LoadBalanceOnDropGuard,
    pub pinner: CorePinner,
}

impl AllocatedJobCore {
    pub fn into_shared(self) -> (Arc<LoadBalanceOnDropGuard>, CorePinner) {
        (Arc::new(self.guard), self.pinner)
    }

    /// Spawn a [`SingleCoreExecutor`] allocated to this core.
    pub fn spawn_executor<S: Send + 'static>(
        guard: Arc<LoadBalanceOnDropGuard>,
        pinner: CorePinner,
        state: S,
        name: impl Into<String>,
    ) -> SingleCoreExecutor<S> {
        SingleCoreExecutor::spawn_and_pin(guard, pinner, state, Some(name.into()))
    }
}

/// Used for pinning a job thread to an appropriate core, as determined by
/// [`JobCores`].
///
/// Obtained from [`AllocatedJobCore.pinner`][AllocatedJobCore::pinner].
/// You can either call [`run()`][Self::run] and poll it from the job thread,
/// or call [`pin_now()`][Self::pin_now] once and then
/// [`pin_if_changed()`][Self::pin_if_changed] in a loop.
#[derive(Default, Clone)]
pub struct CorePinner {
    move_core_rx: Option<watch::Receiver<CoreId>>,
}

impl CorePinner {
    #[inline]
    fn do_pin(move_core_rx: &mut watch::Receiver<CoreId>) {
        let core_id = *move_core_rx.borrow_and_update();
        apply_compute_thread_hint(Some(core_id));
    }

    /// Pin the current thread to the appropriate core.
    pub fn pin_now(&mut self) {
        if let Some(move_core_rx) = &mut self.move_core_rx {
            Self::do_pin(move_core_rx);
        }
    }

    /// Repin the current thread to the new appropriate core, if it's changed
    /// since the last call to `pin_now()` or `pin_if_changed()`.
    pub fn pin_if_changed(&mut self) {
        if let Some(move_core_rx) = &mut self.move_core_rx
            && let Ok(true) = move_core_rx.has_changed()
        {
            Self::do_pin(move_core_rx);
        }
    }

    /// In a loop, wait until [`JobCores`] decides that the current thread
    /// needs to move and then repin to the new core.
    pub async fn run(self) {
        let _not_send = std::marker::PhantomData::<*const ()>;
        if let Some(mut move_core_rx) = self.move_core_rx {
            while move_core_rx.changed().await.is_ok() {
                Self::do_pin(&mut move_core_rx);
            }
        }
    }
}

enum ExecutorJob<S> {
    Async(Box<dyn FnOnce() -> LocalBoxFuture<'static, ()> + Send>),
    Sync(Box<dyn FnOnce(&mut S) + Send>),
}

/// A handle to a single-threaded executor for database work.
///
/// Use [`Self::run_job`] to run futures, and [`Self::run_sync_job`] to run functions.
///
/// This handle is cheaply cloneable.
/// When all handles on this database executor have been dropped,
/// its use of the core to which it is pinned will be released,
/// and other databases may be migrated to that core to balance load.
pub struct SingleCoreExecutor<S> {
    inner: Arc<SingleCoreExecutorInner<S>>,
}

impl<S> Clone for SingleCoreExecutor<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

struct SingleCoreExecutorInner<S> {
    /// The sending end of a channel over which we send jobs.
    job_tx: mpsc::UnboundedSender<ExecutorJob<S>>,
}

impl<S: Send + 'static> SingleCoreExecutor<S> {
    fn spawn_and_pin(
        guard: Arc<LoadBalanceOnDropGuard>,
        mut pinner: CorePinner,
        mut state: S,
        name: Option<String>,
    ) -> Self {
        let (job_tx, mut job_rx) = mpsc::unbounded_channel::<ExecutorJob<S>>();

        let inner = Arc::new(SingleCoreExecutorInner { job_tx });

        let rt = runtime::Handle::current();
        let mut thread = std::thread::Builder::new();
        if let Some(name) = name {
            thread = thread.name(name);
        }
        let worker = move || {
            let _guard = guard;
            pinner.pin_now();

            let _entered = rt.enter();
            let local = tokio::task::LocalSet::new();

            // Clone the pinner: one for inline repinning in the sync job handler,
            // one for the background re-pinning watcher.
            let mut loop_pinner = pinner.clone();

            let job_loop = async {
                while let Some(job) = job_rx.recv().await {
                    match job {
                        ExecutorJob::Async(job) => {
                            local.spawn_local(job());
                        }
                        ExecutorJob::Sync(job) => {
                            loop_pinner.pin_if_changed();
                            job(&mut state);
                        }
                    }
                }
            };

            // Run the pinner on the same task as the job loop, so that the pinner still
            // being alive doesn't prevent the runtime thread from ending.
            rt.block_on(local.run_until(super::also_poll(job_loop, pinner.run())));

            // The sender has closed; finish out any remaining tasks left on the set.
            // This is very important to do - otherwise, in-progress tasks will be
            // dropped and cancelled.
            rt.block_on(local)
        };
        thread.spawn(worker).expect("failed to spawn SingleCoreExecutor thread");

        Self { inner }
    }

    /// Run an async job on this executor and return its result.
    pub async fn run_async_job<F, R>(&self, f: F) -> R
    where
        F: AsyncFnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let span = tracing::Span::current();
        let (tx, rx) = oneshot::channel();

        self.inner
            .job_tx
            .send(ExecutorJob::Async(Box::new(move || {
                async move {
                    let result = AssertUnwindSafe(f().instrument(span)).catch_unwind().await;
                    if let Err(Err(_panic)) = tx.send(result) {
                        tracing::warn!("uncaught panic on `SingleCoreExecutor`")
                    }
                }
                .boxed_local()
            })))
            .unwrap_or_else(|_| panic!("job thread exited"));

        match rx.await.unwrap() {
            Ok(r) => r,
            Err(e) => std::panic::resume_unwind(e),
        }
    }

    /// Enqueue a job for this database executor without waiting for its result.
    pub fn enqueue_async_job<F>(&self, f: F)
    where
        F: AsyncFnOnce() + Send + 'static,
    {
        let span = tracing::Span::current();

        self.inner
            .job_tx
            .send(ExecutorJob::Async(Box::new(move || {
                async move {
                    if AssertUnwindSafe(f().instrument(span)).catch_unwind().await.is_err() {
                        tracing::warn!("uncaught panic on `SingleCoreExecutor`")
                    }
                }
                .boxed_local()
            })))
            .unwrap_or_else(|_| panic!("job thread exited"));
    }

    /// Run `f` on this database executor and return its result.
    pub async fn run_sync_job<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut S) -> R + Send + 'static,
        R: Send + 'static,
    {
        let span = tracing::Span::current();
        let (tx, rx) = oneshot::channel();

        self.inner
            .job_tx
            .send(ExecutorJob::Sync(Box::new(move |state| {
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    let _entered = span.enter();
                    f(state)
                }));
                if let Err(Err(_panic)) = tx.send(result) {
                    tracing::warn!("uncaught panic on `SingleCoreExecutor`")
                }
            })))
            .unwrap_or_else(|_| panic!("job thread exited"));

        match rx.await.unwrap() {
            Ok(r) => r,
            Err(e) => std::panic::resume_unwind(e),
        }
    }

    /// Enqueue a job for this database executor without waiting for its result.
    pub fn enqueue_sync_job<F>(&self, f: F)
    where
        F: FnOnce(&mut S) + Send + 'static,
    {
        let span = tracing::Span::current();

        self.inner
            .job_tx
            .send(ExecutorJob::Sync(Box::new(move |state| {
                if std::panic::catch_unwind(AssertUnwindSafe(|| {
                    let _entered = span.enter();
                    f(state);
                }))
                .is_err()
                {
                    tracing::warn!("uncaught panic on `SingleCoreExecutor`")
                }
            })))
            .unwrap_or_else(|_| panic!("job thread exited"));
    }
}

/// On drop, tells the [`JobCores`] that this database is no longer occupying its core,
/// allowing databases from more-contended runtimes/cores to migrate there.
#[derive(Default)]
pub struct LoadBalanceOnDropGuard {
    inner: Option<(Weak<Mutex<PinnedCoresExecutorManager>>, SingleCoreExecutorId)>,
}

impl Drop for LoadBalanceOnDropGuard {
    fn drop(&mut self) {
        if let Some((manager, database_executor_id)) = &self.inner
            && let Some(cores) = manager.upgrade()
        {
            cores.lock().unwrap().deallocate(*database_executor_id);
        }
    }
}
