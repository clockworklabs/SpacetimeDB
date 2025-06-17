use std::sync::{Arc, Mutex, Weak};

use core_affinity::CoreId;
use indexmap::IndexMap;
use smallvec::SmallVec;
use spacetimedb_data_structures::map::HashMap;
use tokio::sync::{mpsc, oneshot, watch};

use super::notify_once::NotifyOnce;

/// A handle to a pool of CPU cores for running job threads on.
///
/// Each thread is represented by a [`JobThread`], which is pinned to a single
/// core and sequentially runs the jobs that are passed to [`JobThread::run`].
/// This pool attempts to keep the number of `JobThread`s pinned to each core
/// as equitable as possible; new threads allocated by [`Self::take()`] are
/// assigned to cores in a round-robin fashion, and when a thread exits, it
/// takes a thread pinned to a busier core and repins it to the core it was
/// just running on.
///
/// Construction is done via the `FromIterator` impl. If created from an empty
/// iterator or via `JobCores::default()`, the job threads will work but not be
/// pinned to any threads.
///
/// This handle is cheaply cloneable. If all instances of it are dropped,
/// threads will continue running, but will no longer repin each other
/// when one exits.
#[derive(Default, Clone)]
pub struct JobCores {
    inner: Option<Arc<Mutex<JobCoresInner>>>,
}

struct JobCoresInner {
    /// A map to the repin_tx for each job thread
    job_threads: HashMap<JobThreadId, watch::Sender<CoreId>>,
    cores: IndexMap<CoreId, CoreInfo>,
    /// An index into `cores` of the next core to put a new job onto.
    ///
    /// This acts as a partition point in `cores`; all cores in `..index` have
    /// one fewer job on them than the cores in `index..`.
    next_core: usize,
    next_id: JobThreadId,
}

#[derive(Default)]
struct CoreInfo {
    jobs: SmallVec<[JobThreadId; 4]>,
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct JobThreadId(usize);

impl JobCores {
    /// Reserve a core from the pool to later start a job thread on.
    pub fn take(&self) -> JobCore {
        let inner = if let Some(inner) = &self.inner {
            let cores = Arc::downgrade(inner);
            let (id, repin_rx) = inner.lock().unwrap().allocate();
            Some(JobCoreInner {
                repin_rx,
                _guard: JobCoreGuard { cores, id },
            })
        } else {
            None
        };

        JobCore { inner }
    }
}

impl FromIterator<CoreId> for JobCores {
    fn from_iter<T: IntoIterator<Item = CoreId>>(iter: T) -> Self {
        let cores: IndexMap<_, _> = iter.into_iter().map(|id| (id, CoreInfo::default())).collect();
        let inner = (!cores.is_empty()).then(|| {
            Arc::new(Mutex::new(JobCoresInner {
                job_threads: HashMap::default(),
                cores,
                next_core: 0,
                next_id: JobThreadId(0),
            }))
        });
        Self { inner }
    }
}

impl JobCoresInner {
    fn allocate(&mut self) -> (JobThreadId, watch::Receiver<CoreId>) {
        let id = self.next_id;
        self.next_id.0 += 1;

        let (&core_id, core) = self.cores.get_index_mut(self.next_core).unwrap();
        core.jobs.push(id);
        self.next_core = (self.next_core + 1) % self.cores.len();

        let (repin_tx, repin_rx) = watch::channel(core_id);
        self.job_threads.insert(id, repin_tx);

        (id, repin_rx)
    }

    /// Run when a `JobThread` exits.
    fn deallocate(&mut self, id: JobThreadId) {
        let core_id = *self.job_threads.remove(&id).unwrap().borrow();

        let core_index = self.cores.get_index_of(&core_id).unwrap();

        // This core is now less busy than it should be - bump `next_core` back
        // by 1 and steal a thread from the core there.
        //
        // This wraps around in the 0 case, so the partition point is simply
        // moved to the end of the ring buffer.

        let steal_from_index = self.next_core.checked_sub(1).unwrap_or(self.cores.len() - 1);

        // if this core was already at `next_core - 1`, we don't need to steal from anywhere
        let (core, steal_from) = match self.cores.get_disjoint_indices_mut([core_index, steal_from_index]) {
            Ok([(_, core), (_, steal_from)]) => (core, Some(steal_from)),
            Err(_) => (&mut self.cores[core_index], None),
        };

        let pos = core.jobs.iter().position(|x| *x == id).unwrap();
        core.jobs.remove(pos);

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
            core.jobs.push(stolen);
            self.job_threads[&stolen].send_replace(core_id);
        }

        self.next_core = steal_from_index;
    }
}

/// A core taken from [`JobCores`], not yet running a job loop.
#[derive(Default)]
pub struct JobCore {
    inner: Option<JobCoreInner>,
}

struct JobCoreInner {
    repin_rx: watch::Receiver<CoreId>,
    _guard: JobCoreGuard,
}

impl JobCore {
    /// Start running a job thread on this core.
    ///
    /// `init` constructs the data provided to each job, and `unsize` unsizes
    /// it to `&mut T`, if necessary.
    pub fn start<F, F2, U, T>(self, init: F, unsize: F2) -> JobThread<T>
    where
        F: FnOnce() -> U + Send + 'static,
        F2: FnOnce(&mut U) -> &mut T + Send + 'static,
        U: 'static,
        T: ?Sized + 'static,
    {
        let (tx, rx) = mpsc::channel::<Box<Job<T>>>(Self::JOB_CHANNEL_LENGTH);
        let close = Arc::new(NotifyOnce::new());

        let closed = close.clone();
        let handle = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let mut data = init();
            let data = unsize(&mut data);
            handle.block_on(self.job_loop(rx, closed, data))
        });

        JobThread { tx, close }
    }

    // this shouldn't matter too much, since callers will need to wait for
    // the job to finish anyway.
    const JOB_CHANNEL_LENGTH: usize = 64;

    async fn job_loop<T: ?Sized>(mut self, mut rx: mpsc::Receiver<Box<Job<T>>>, closed: Arc<NotifyOnce>, data: &mut T) {
        // this function is async because we need to recv on the repin channel
        // and the jobs channel, but the jobs being run are blocking

        let repin_rx = self.inner.as_mut().map(|inner| &mut inner.repin_rx);
        let repin_loop = async {
            if let Some(rx) = repin_rx {
                rx.mark_changed();
                while rx.changed().await.is_ok() {
                    core_affinity::set_for_current(*rx.borrow_and_update());
                }
            }
        };

        let job_loop = async {
            while let Some(job) = rx.recv().await {
                job(data)
            }
        };

        tokio::select! {
            () = super::also_poll(job_loop, repin_loop) => {}
            // when we receive a close notification, we immediately drop all
            // remaining jobs in the queue.
            () = closed.notified() => {}
        }
    }
}

/// On drop, tells the `JobCores` that this core has been freed up.
struct JobCoreGuard {
    cores: Weak<Mutex<JobCoresInner>>,
    id: JobThreadId,
}

impl Drop for JobCoreGuard {
    fn drop(&mut self) {
        if let Some(cores) = self.cores.upgrade() {
            cores.lock().unwrap().deallocate(self.id);
        }
    }
}

/// A handle to a thread running a job loop; see [`JobCores`] for more details.
///
/// The thread stores data of type `T`; jobs run on the thread will be given
/// mutable access to it.
///
/// This handle is cheaply cloneable. If all strong handles have been dropped,
/// the thread will shut down.
pub struct JobThread<T: ?Sized> {
    tx: mpsc::Sender<Box<Job<T>>>,
    close: Arc<NotifyOnce>,
}

impl<T: ?Sized> Clone for JobThread<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            close: self.close.clone(),
        }
    }
}

type Job<T> = dyn FnOnce(&mut T) + Send;

impl<T: ?Sized> JobThread<T> {
    /// Run a blocking job on this `JobThread`.
    ///
    /// The job (`f`) will be placed in a queue, and will run strictly after
    /// jobs ahead of it in the queue. If `f` panics, it will be bubbled up to
    /// the calling task.
    pub async fn run<F, R>(&self, f: F) -> Result<R, JobThreadClosed>
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (ret_tx, ret_rx) = oneshot::channel();

        let span = tracing::Span::current();
        self.tx
            .send(Box::new(move |data| {
                let _entered = span.entered();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(data)));
                if let Err(Err(_panic)) = ret_tx.send(result) {
                    tracing::warn!("uncaught panic on threadpool")
                }
            }))
            .await
            .map_err(|_| JobThreadClosed)?;

        match ret_rx.await {
            Ok(Ok(ret)) => Ok(ret),
            Ok(Err(panic)) => std::panic::resume_unwind(panic),
            Err(_closed) => Err(JobThreadClosed),
        }
    }

    /// Shutdown the job thread.
    pub fn close(&self) {
        self.close.notify();
    }

    /// Returns a future that resolves once the job thread has been closed.
    pub async fn closed(&self) {
        self.tx.closed().await
    }

    /// Obtain a weak version of this handle.
    pub fn downgrade(&self) -> WeakJobThread<T> {
        let tx = self.tx.downgrade();
        let close = Arc::downgrade(&self.close);
        WeakJobThread { tx, close }
    }
}

pub struct JobThreadClosed;

/// A weak version of `JobThread` that does not hold the thread open.
// used in crate::core::module_host::WeakModuleHost
pub struct WeakJobThread<T: ?Sized> {
    tx: mpsc::WeakSender<Box<Job<T>>>,
    close: Weak<NotifyOnce>,
}

impl<T: ?Sized> WeakJobThread<T> {
    pub fn upgrade(&self) -> Option<JobThread<T>> {
        Option::zip(self.tx.upgrade(), self.close.upgrade()).map(|(tx, close)| JobThread { tx, close })
    }
}
