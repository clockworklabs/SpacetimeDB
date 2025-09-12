use std::future::Future;
use std::sync::{Arc, Mutex, Weak};

use core_affinity::CoreId;
use indexmap::IndexMap;
use smallvec::SmallVec;
use spacetimedb_data_structures::map::HashMap;
use tokio::runtime;
use tokio::sync::watch;

/// A handle to a pool of Tokio executors for running database WASM code on.
///
/// Each database has a [`DatabaseExecutor`],
/// a handle to a single-threaded Tokio runtime which is pinned to a specific CPU core.
/// In multi-tenant environments, multiple databases' [`DatabaseExecutor`]s may be handles on the same runtime/core,
/// and a [`DatabaseExecutor`] may occasionally be migrated to a different runtime/core to balance load.
///
/// Construct a `JobCores` via [`Self::from_pinned_cores`] or [`Self::without_pinned_cores`].
/// a `JobCores` constructed without core pinning, including `from_pinned_cores` on an empty set,
/// will use the "global" Tokio executor to run database jobs,
/// rather than creating multiple un-pinned single-threaded runtimes.
///
/// This handle is cheaply cloneable, but at least one handle must be kept alive.
/// If all instances of it are dropped, the per-thread [`runtime::Runtime`]s will be dropped,
/// and so will stop executing jobs for databases.
#[derive(Clone)]
pub struct JobCores {
    inner: JobCoresInner,
}

#[derive(Clone)]
enum JobCoresInner {
    PinnedCores(Arc<Mutex<PinnedCoresExecutorManager>>),
    NoPinning(runtime::Handle),
}

struct PinnedCoresExecutorManager {
    /// Channels to request that a [`DatabaseExecutor`] move to a different Tokio runtime.
    ///
    /// Alongside each channel is the [`CoreId`] of the runtime to which that [`DatabaseExecutor`] is currently pinned.
    /// This is used as an index into `self.cores` to make load-balancing decisions when freeing a database executor
    /// in [`Self::deallocate`].
    database_executor_move: HashMap<DatabaseExecutorId, (CoreId, watch::Sender<runtime::Handle>)>,
    cores: IndexMap<CoreId, CoreInfo>,
    /// An index into `cores` of the next core to put a new job onto.
    ///
    /// This acts as a partition point in `cores`; all cores in `..index` have
    /// one fewer job on them than the cores in `index..`.
    next_core: usize,
    next_id: DatabaseExecutorId,
}

/// Stores the [`tokio::Runtime`] pinned to a particular core,
/// and remembers the [`DatabaseExecutorId`]s for all databases sharing that executor.
struct CoreInfo {
    jobs: SmallVec<[DatabaseExecutorId; 4]>,
    tokio_runtime: runtime::Runtime,
}

impl CoreInfo {
    fn spawn_executor(id: CoreId) -> CoreInfo {
        let runtime = runtime::Builder::new_multi_thread()
            .worker_threads(1)
            // [`DatabaseExecutor`]s should only be executing Wasmtime WASM futures,
            // and so should never be doing [`Tokio::spawn_blocking`] or performing blocking I/O.
            // However, `max_blocking_threads` will panic if passed 0, so we set a limit of 1
            // and use `on_thread_start` to log an error when spawning a blocking task.
            .max_blocking_threads(1)
            .on_thread_start({
                use std::sync::atomic::{AtomicBool, Ordering};
                let already_spawned_worker = AtomicBool::new(false);
                move || {
                    // `Ordering::Relaxed`: No synchronization is happening here;
                    // we're not writing to any other memory or coordinating with any other atomic places.
                    // We rely on Tokio's infrastructure to impose a happens-before relationship
                    // between spawning worker threads and spawning blocking threads itself.
                    if already_spawned_worker.swap(true, Ordering::Relaxed) {
                        // We're spawning a blocking thread, naughty!
                        log::error!(
                            "`JobCores` Tokio runtime for `DatabaseExecutor` use on core {id:?} spawned a blocking thread!"
                        );
                    } else {
                        // We're spawning our 1 worker, so pin it to the appropriate thread.
                        core_affinity::set_for_current(id);
                    }
                }
            })
            .build()
            .expect("Failed to start Tokio executor for `DatabaseExecutor`");
        CoreInfo {
            jobs: SmallVec::new(),
            tokio_runtime: runtime,
        }
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct DatabaseExecutorId(usize);

impl JobCores {
    /// Get a handle on a [`DatabaseExecutor`] to later run a database's jobs on.
    pub fn take(&self) -> DatabaseExecutor {
        let database_executor_inner = match &self.inner {
            JobCoresInner::NoPinning(handle) => DatabaseExecutorInner {
                runtime: watch::channel(handle.clone()).1,
                _guard: None,
            },
            JobCoresInner::PinnedCores(manager) => {
                let manager_weak = Arc::downgrade(manager);
                let (database_executor_id, move_runtime_rx) = manager.lock().unwrap().allocate();
                DatabaseExecutorInner {
                    runtime: move_runtime_rx,
                    _guard: Some(LoadBalanceOnDropGuard {
                        manager: manager_weak,
                        database_executor_id,
                    }),
                }
            }
        };
        DatabaseExecutor {
            inner: Arc::new(database_executor_inner),
        }
    }

    /// Construct a [`JobCores`] which runs one Tokio runtime on each of the `cores`,
    /// and pins each database to a particular runtime/core.
    ///
    /// If `cores` is empty, this falls back to [`Self::without_pinned_cores`]
    /// and runs all databases in the `global_runtime`.
    pub fn from_pinned_cores(cores: impl IntoIterator<Item = CoreId>, global_runtime: runtime::Handle) -> Self {
        let cores: IndexMap<_, _> = cores.into_iter().map(|id| (id, CoreInfo::spawn_executor(id))).collect();
        let inner = if cores.is_empty() {
            JobCoresInner::NoPinning(global_runtime)
        } else {
            JobCoresInner::PinnedCores(Arc::new(Mutex::new(PinnedCoresExecutorManager {
                database_executor_move: HashMap::default(),
                cores,
                next_core: 0,
                next_id: DatabaseExecutorId(0),
            })))
        };

        Self { inner }
    }

    /// Construct a [`JobCores`] which does not perform any core pinning,
    /// and just runs all database jobs in `global_runtime`.
    ///
    /// This will be used in deployments where there aren't enough available CPU cores
    /// to reserve specific cores for database WASM execution.
    pub fn without_pinned_cores(global_runtime: runtime::Handle) -> Self {
        Self {
            inner: JobCoresInner::NoPinning(global_runtime),
        }
    }
}

impl PinnedCoresExecutorManager {
    fn allocate(&mut self) -> (DatabaseExecutorId, watch::Receiver<runtime::Handle>) {
        let database_executor_id = self.next_id;
        self.next_id.0 += 1;

        let (&core_id, runtime_handle) = {
            let (core_id, core_info) = self.cores.get_index_mut(self.next_core).unwrap();
            core_info.jobs.push(database_executor_id);
            (core_id, core_info.tokio_runtime.handle().clone())
        };
        self.next_core = (self.next_core + 1) % self.cores.len();

        let (move_runtime_tx, move_runtime_rx) = watch::channel(runtime_handle);
        self.database_executor_move
            .insert(database_executor_id, (core_id, move_runtime_tx));

        (database_executor_id, move_runtime_rx)
    }

    /// Run when a `JobThread` exits.
    fn deallocate(&mut self, id: DatabaseExecutorId) {
        let (freed_core_id, _) = self.database_executor_move.remove(&id).unwrap();

        let core_index = self.cores.get_index_of(&freed_core_id).unwrap();

        // This core is now less busy than it should be - bump `next_core` back
        // by 1 and steal a thread from the core there.
        //
        // This wraps around in the 0 case, so the partition point is simply
        // moved to the end of the ring buffer.

        let steal_from_index = self.next_core.checked_sub(1).unwrap_or(self.cores.len() - 1);

        // if this core was already at `next_core - 1`, we don't need to steal from anywhere
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
            let (ref mut stolen_core_id, migrate_tx) = self.database_executor_move.get_mut(&stolen).unwrap();
            *stolen_core_id = freed_core_id;
            migrate_tx.send_replace(core_info.tokio_runtime.handle().clone());
        }

        self.next_core = steal_from_index;
    }
}

/// A handle to a Tokio executor which can be used to run WASM compute for a particular database.
///
/// Use [`Self::run_job`] to run futures, and [`Self::run_sync_job`] to run functions.
///
/// This handle is cheaply cloneable.
/// When all handles on this database executor have been dropped,
/// its use of the core to which it is pinned will be released,
/// and other databases may be migrated to that core to balance load.
pub struct DatabaseExecutor {
    inner: Arc<DatabaseExecutorInner>,
}

struct DatabaseExecutorInner {
    /// Handle on the [`runtime::Runtime`] where this executor should run jobs.
    ///
    /// This will be occasionally updated by [`PinnedCoresExecutorManager::deallocate`]
    /// to evenly distribute databases across the available runtimes/cores.
    runtime: watch::Receiver<runtime::Handle>,

    /// [`Drop`] guard which calls [`PinnedCoresExecutorManager::deallocate`] when this database dies,
    /// allowing another database from a more-contended runtime/core to migrate here.
    _guard: Option<LoadBalanceOnDropGuard>,
}

impl DatabaseExecutor {
    /// Run a job for this database executor.
    ///
    /// `f` must not perform any `Tokio::spawn_blocking` blocking operations.
    pub async fn run_job<F, R>(&self, f: F) -> R
    where
        F: Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        match self.inner.runtime.borrow().spawn(f).await {
            Ok(r) => r,
            Err(e) => std::panic::resume_unwind(e.into_panic()),
        }
    }

    /// Run `f` on this database executor and return its result.
    pub async fn run_sync_job<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.run_job(async { f() }).await
    }
}

/// On drop, tells the [`JobCores`] that this database is no longer occupying its Tokio runtime,
/// allowing databases from more-contended runtimes/cores to migrate there.
struct LoadBalanceOnDropGuard {
    manager: Weak<Mutex<PinnedCoresExecutorManager>>,
    database_executor_id: DatabaseExecutorId,
}

impl Drop for LoadBalanceOnDropGuard {
    fn drop(&mut self) {
        if let Some(cores) = self.manager.upgrade() {
            cores.lock().unwrap().deallocate(self.database_executor_id);
        }
    }
}

/// A weak version of `JobThread` that does not hold the thread open.
// used in crate::core::module_host::WeakModuleHost
pub struct WeakDatabaseExecutor {
    inner: Weak<DatabaseExecutorInner>,
}

impl WeakDatabaseExecutor {
    pub fn upgrade(&self) -> Option<DatabaseExecutor> {
        self.inner.upgrade().map(|inner| DatabaseExecutor { inner })
    }
}
