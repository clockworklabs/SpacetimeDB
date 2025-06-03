use std::cmp;
use std::sync::{Arc, Mutex, Weak};

use core_affinity::CoreId;
use indexmap::IndexMap;
use smallvec::SmallVec;
use spacetimedb_data_structures::map::HashMap;
use tokio::sync::{mpsc, oneshot, watch};

/// A pool of CPU cores for running jobs on.
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
#[derive(Default, Clone)]
pub struct JobCores {
    inner: Option<Arc<Mutex<JobCoresInner>>>,
}

struct JobCoresInner {
    /// A map to the repin_tx for each job thread
    job_threads: HashMap<JobThreadId, watch::Sender<CoreId>>,
    /// Kept sorted by `CoreInfo.jobs.len()`, in ascending order
    cores: IndexMap<CoreId, CoreInfo>,
    next_id: JobThreadId,
}

#[derive(Default)]
struct CoreInfo {
    jobs: SmallVec<[JobThreadId; 4]>,
}
fn cores_cmp(_: &CoreId, v1: &CoreInfo, _: &CoreId, v2: &CoreInfo) -> cmp::Ordering {
    Ord::cmp(&v1.jobs.len(), &v2.jobs.len())
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct JobThreadId(usize);

impl JobCores {
    /// Reserve a core from the pool to later start a job thread on.
    pub fn take(&self) -> JobCore {
        let inner = if let Some(inner) = &self.inner {
            let cores = Arc::downgrade(inner);
            let mut inner = inner.lock().unwrap();

            let id = inner.next_id;
            inner.next_id.0 += 1;

            let (&core_id, core) = inner.cores.first_mut().unwrap();
            core.jobs.push(id);
            inner.cores.sort_by(cores_cmp);

            let (repin_tx, repin_rx) = watch::channel(core_id);
            inner.job_threads.insert(id, repin_tx);

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
                next_id: JobThreadId(0),
            }))
        });
        Self { inner }
    }
}

impl JobCoresInner {
    /// Run when a `JobThread` exits.
    fn on_thread_exit(&mut self, id: JobThreadId) {
        let core_id = *self.job_threads.remove(&id).unwrap().borrow();

        let core_index = self.cores.get_index_of(&core_id).unwrap();
        let last_index = self.cores.len() - 1;

        // `last_core` will be Some if `core_index` is not `last_index`
        // FIXME(noa): this will fail to level sometimes; we should keep a partition point and
        //     manually move cores before it when they're low and above it when they're high.
        let (core, last_core) = match self.cores.get_disjoint_indices_mut([core_index, last_index]) {
            Ok([(_, core), (_, last)]) => (core, Some(last)),
            Err(_) => (&mut self.cores[core_index], None),
        };

        let job_pos = core.jobs.iter().position(|x| *x == id).unwrap();

        if let Some(job) = last_core.and_then(|last| last.jobs.pop()) {
            core.jobs[job_pos] = job;
            let sender = self.job_threads.get_mut(&job).unwrap();
            sender.send_replace(core_id);
        } else {
            core.jobs.remove(job_pos);
        }
        self.cores.sort_by(cores_cmp);
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

        let handle = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let mut data = init();
            let data = unsize(&mut data);
            handle.block_on(self.job_loop(rx, data))
        });

        JobThread { tx }
    }

    // this shouldn't matter too much, since callers will need to wait for
    // the job to finish anyway.
    const JOB_CHANNEL_LENGTH: usize = 50;

    async fn job_loop<T: ?Sized>(mut self, mut rx: mpsc::Receiver<Box<Job<T>>>, data: &mut T) {
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
                // blocking in place means that other futures on the same task
                // won't get polled - in this case, that's just the repin loop,
                // which is fine because it can just run before the next job.
                tokio::task::block_in_place(|| job(data))
            }
        };

        super::also_poll(job_loop, repin_loop).await
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
            cores.lock().unwrap().on_thread_exit(self.id);
        }
    }
}

/// A handle to a thread running a job loop; see [`JobCores`] for more details.
///
/// This handle is cheaply cloneäble, and the thread will shut down when all
/// handles to it have been dropped.
pub struct JobThread<T: ?Sized> {
    tx: mpsc::Sender<Box<Job<T>>>,
}

impl<T: ?Sized> Clone for JobThread<T> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

type Job<T> = dyn FnOnce(&mut T) + Send;

impl<T: ?Sized> JobThread<T> {
    /// Run a blocking job on this thread.
    ///
    /// The job (`f`) will be placed in a queue, and will run strictly after
    /// jobs ahead of it in the queue. If `f` panics, it will be bubbled up to
    /// the calling task.
    pub async fn run<F, R>(&self, f: F) -> R
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
            .expect("job thread terminated unexpectedly");

        ret_rx.await.unwrap().unwrap_or_else(|e| std::panic::resume_unwind(e))
    }

    /// Obtain a weak version of this handle.
    pub fn downgrade(&self) -> WeakJobThread<T> {
        let tx = self.tx.downgrade();
        WeakJobThread { tx }
    }
}

/// A weak version of `JobThread` that does not hold the thread open.
pub struct WeakJobThread<T: ?Sized> {
    tx: mpsc::WeakSender<Box<Job<T>>>,
}

impl<T: ?Sized> WeakJobThread<T> {
    pub fn upgrade(&self) -> Option<JobThread<T>> {
        self.tx.upgrade().map(|tx| JobThread { tx })
    }
}
