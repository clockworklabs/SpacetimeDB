use std::cmp;
use std::sync::{Arc, Mutex, Weak};

use core_affinity::CoreId;
use indexmap::IndexMap;
use smallvec::SmallVec;
use spacetimedb_data_structures::map::HashMap;
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Default, Clone)]
pub struct JobCores {
    inner: Option<Arc<Mutex<JobCoresInner>>>,
}

struct JobCoresInner {
    job_threads: HashMap<JobThreadId, watch::Sender<CoreId>>,
    cores: IndexMap<CoreId, CoreInfo>,
    next_id: usize,
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
    pub fn take(&self) -> JobCore {
        let inner = if let Some(inner) = &self.inner {
            let cores = Arc::downgrade(inner);
            let mut inner = inner.lock().unwrap();
            let id = JobThreadId(inner.next_id);
            inner.next_id += 1;
            let (&core_id, core) = inner.cores.first_mut().unwrap();
            core.jobs.push(id);
            inner.cores.sort_by(cores_cmp);

            let (repin_tx, repin_rx) = watch::channel(core_id);
            inner.job_threads.insert(id, repin_tx);

            Some(JobCoreInner { repin_rx, cores, id })
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
                next_id: 0,
            }))
        });
        Self { inner }
    }
}

impl JobCoresInner {
    fn thread_exited(&mut self, id: JobThreadId) {
        let core_id = *self.job_threads.remove(&id).unwrap().borrow();
        let core_index = self.cores.get_index_of(&core_id).unwrap();
        let last_index = self.cores.len() - 1;
        let (core, last_core) = match self.cores.get_disjoint_indices_mut([core_index, last_index]) {
            Ok([(_, core), (_, last)]) => (core, Some(last)),
            Err(_) => (&mut self.cores[core_index], None),
        };
        let pos = core.jobs.iter().position(|x| *x == id).unwrap();
        core.jobs.remove(pos);
        if let Some(job) = last_core.and_then(|last| last.jobs.pop()) {
            core.jobs[pos] = job;
            let sender = self.job_threads.get_mut(&job).unwrap();
            sender.send_replace(core_id);
        } else {
            core.jobs.remove(pos);
        }
        self.cores.sort_by(cores_cmp);
    }
}

#[derive(Default)]
pub struct JobCore {
    inner: Option<JobCoreInner>,
}

struct JobCoreInner {
    repin_rx: watch::Receiver<CoreId>,
    cores: Weak<Mutex<JobCoresInner>>,
    id: JobThreadId,
}

impl JobCore {
    pub fn start<F, F2, U, T>(self, init: F, as_t: F2) -> JobThread<T>
    where
        F: FnOnce() -> U + Send + 'static,
        F2: FnOnce(&mut U) -> &mut T + Send + 'static,
        U: 'static,
        T: ?Sized + 'static,
    {
        let (tx, rx) = mpsc::channel::<Box<Job<T>>>(8);

        let handle = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let mut data = init();
            let data = as_t(&mut data);
            self.job_loop(&handle, rx, data);
        });

        JobThread { tx }
    }

    fn job_loop<T: ?Sized>(self, handle: &tokio::runtime::Handle, mut rx: mpsc::Receiver<Box<Job<T>>>, data: &mut T) {
        let (mut repin_rx, cores_id) = self
            .inner
            .map(|inner| (inner.repin_rx, (inner.cores, inner.id)))
            .unzip();
        if let Some(repin_rx) = &mut repin_rx {
            core_affinity::set_for_current(*repin_rx.borrow_and_update());
        }
        scopeguard::defer!({
            if let Some((cores, id)) = cores_id {
                if let Some(cores) = cores.upgrade() {
                    cores.lock().unwrap().thread_exited(id);
                }
            }
        });
        handle.block_on(async {
            loop {
                let repin_fut = async {
                    let rx = repin_rx.as_mut()?;
                    rx.changed().await.ok()?;
                    Some(*rx.borrow_and_update())
                };
                tokio::select! {
                    Some(core_id) = repin_fut => {
                        core_affinity::set_for_current(core_id);
                    }
                    job = rx.recv() => {
                        let Some(job) = job else { break };
                        tokio::task::block_in_place(|| job(data))
                    }
                }
            }
        });
    }
}

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
    pub async fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        R: Send + 'static,
    {
        let span = tracing::Span::current();
        let (ret_tx, ret_rx) = oneshot::channel();
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

    pub fn downgrade(&self) -> WeakJobThread<T> {
        let tx = self.tx.downgrade();
        WeakJobThread { tx }
    }
}

pub struct WeakJobThread<T: ?Sized> {
    tx: mpsc::WeakSender<Box<Job<T>>>,
}

impl<T: ?Sized> WeakJobThread<T> {
    pub fn upgrade(&self) -> Option<JobThread<T>> {
        self.tx.upgrade().map(|tx| JobThread { tx })
    }
}
