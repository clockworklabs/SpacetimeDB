use alloc::{boxed::Box, collections::BTreeMap, format, sync::Arc, vec::Vec};
use core::{
    cell::UnsafeCell,
    fmt,
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    task::{Context, Poll, Waker},
    time::Duration,
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
    sync::{Condvar, Mutex as StdMutex, MutexGuard as StdMutexGuard},
    thread::{self, JoinHandle as ThreadJoinHandle},
};

use spin::Mutex;

use crate::sim::{time::TimeHandle, Rng};

mod task;
use task::Abortable;
pub use task::{AbortHandle, JoinError, JoinHandle};

type Runnable = async_task::Runnable<NodeId>;

/// A synchronous closure submitted through simulated `spawn_blocking`.
///
/// The closure is `Send` because it may run on any simulated worker thread.
/// The scheduler still grants a run permit to only one worker at a time, so
/// these jobs are stackful but not actually parallel.
type BlockingJob = Box<dyn FnOnce() + Send + 'static>;

/// Identifier for one OS thread owned by the simulator.
///
/// A worker is used as a parked Rust stack. It can be idle and reusable after
/// polling an async task, or occupied when synchronous code parks inside
/// `yield_sync` or a blocking primitive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct SimWorkerId(u64);

/// Monotonic identifier for a single driver-granted execution slice.
///
/// A stackful worker can yield or block several times while keeping the same
/// synchronous stack alive. Correlating every report with the run id prevents a
/// stale report from satisfying a later permit for the same worker.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RunId(u64);

/// Identifier for a scheduler-visible [`SimMutex`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct MutexId(u64);

/// A live synchronous stack parked on one worker.
///
/// The `run_id` is part of the identity: a later run on the same worker is not
/// allowed to satisfy a permit intended for this parked stack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ParkedStack {
    worker: SimWorkerId,
    run_id: RunId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MutexWaiter {
    worker: SimWorkerId,
    run_id: RunId,
}

/// Reason a worker parked without becoming immediately runnable.
///
/// Unlike `yield_sync`, a blocked worker must not be scheduled again until the
/// resource it is waiting on explicitly makes it runnable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlockReason {
    /// The worker is waiting for ownership of a [`SimMutex`].
    Mutex(MutexId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParkedReadiness {
    Ready,
    Blocked(BlockReason),
}

/// One item in the deterministic scheduler's ready queue.
///
/// This queue is the only place where the driver chooses what progresses next.
/// Condvars move permits and reports between threads, but they never decide
/// ordering; selection from this enum is driven by the runtime RNG.
enum RunnableEntity {
    /// A `Send` async task runnable. The driver polls it on a reusable worker.
    Async(Runnable),
    /// A non-`Send` runnable. These stay on the caller/local thread path to
    /// preserve the existing `spawn_local` and top-level `block_on` contracts.
    LocalAsync(Runnable),
    /// A worker whose stack is parked inside synchronous code and can resume.
    Parked(ParkedStack),
    /// A stackful blocking closure submitted by simulated `spawn_blocking`.
    Blocking(BlockingJob),
}

#[derive(Debug)]
enum WorkerState {
    Idle,
    Running { run_id: RunId },
    Parked { run_id: RunId, readiness: ParkedReadiness },
    Panicked { run_id: RunId },
}

impl WorkerState {
    fn describe(&self) -> alloc::string::String {
        match self {
            Self::Idle => "Idle".into(),
            Self::Running { run_id } => format!("Running(run_id={run_id:?})"),
            Self::Parked { run_id, readiness } => {
                format!("Parked(run_id={run_id:?}, readiness={readiness:?})")
            }
            Self::Panicked { run_id } => format!("Panicked(run_id={run_id:?})"),
        }
    }
}

/// A command from the deterministic driver to one worker thread.
///
/// Workers sleep on their own condvar until the driver installs one of these
/// permits. The permit is single-use: once consumed, the worker runs until it
/// reports back or parks inside synchronous code.
enum WorkerPermit {
    /// Poll this async runnable once by calling `runnable.run()`.
    RunAsync { run_id: RunId, runnable: Runnable },
    /// Execute this simulated blocking closure.
    RunBlocking { run_id: RunId, job: BlockingJob },
    /// Continue from the last stackful park point.
    ResumeParked { run_id: RunId },
    /// Ask an idle worker to exit.
    Shutdown,
}

impl WorkerPermit {
    fn run_id(&self) -> RunId {
        match self {
            Self::RunAsync { run_id, .. } | Self::RunBlocking { run_id, .. } | Self::ResumeParked { run_id } => *run_id,
            Self::Shutdown => panic!("shutdown permits do not have run ids"),
        }
    }

    fn kind_name(&self) -> &'static str {
        match self {
            Self::RunAsync { .. } => "RunAsync",
            Self::RunBlocking { .. } => "RunBlocking",
            Self::ResumeParked { .. } => "ResumeParked",
            Self::Shutdown => "Shutdown",
        }
    }
}

/// How a worker returned control to the deterministic driver.
///
/// Every worker sends exactly one report after each permit before it becomes
/// idle, parked, or blocked. The driver waits for this report before granting
/// any other worker permission to execute user code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerReportKind {
    /// `runnable.run()` returned; the worker's stack is free for reuse.
    PollReturned,
    /// `yield_sync()` parked the worker and made it immediately runnable again.
    Yielded,
    /// A blocking primitive parked the worker until a resource wakes it.
    Blocked(BlockReason),
    /// A `spawn_blocking` closure returned; the worker is reusable.
    FinishedBlocking,
    /// User code panicked while the worker was executing a permit.
    Panicked,
}

/// Report sent from a worker to the driver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorkerReport {
    worker: SimWorkerId,
    run_id: RunId,
    kind: WorkerReportKind,
}

/// Per-worker inbound control channel.
///
/// The driver writes one [`WorkerPermit`] and notifies `cv`; the worker consumes
/// the permit, writes one [`WorkerReport`] into the same slot, and notifies the
/// same condvar. Keeping the rendezvous per-worker avoids a global report queue.
struct WorkerControl {
    slot: StdMutex<WorkerSlot>,
    cv: Condvar,
}

struct WorkerSlot {
    permit: Option<WorkerPermit>,
    report: Option<WorkerReport>,
}

impl WorkerControl {
    fn new() -> Self {
        Self {
            slot: StdMutex::new(WorkerSlot {
                permit: None,
                report: None,
            }),
            cv: Condvar::new(),
        }
    }
}

struct WorkerRecord {
    control: Arc<WorkerControl>,
    state: WorkerState,
    join: Option<ThreadJoinHandle<()>>,
}

impl WorkerRecord {
    fn grant(&mut self, worker: SimWorkerId, permit: WorkerPermit) {
        let run_id = permit.run_id();
        match (&self.state, &permit) {
            (WorkerState::Idle, WorkerPermit::RunAsync { .. } | WorkerPermit::RunBlocking { .. }) => {
                self.state = WorkerState::Running { run_id };
            }
            (
                WorkerState::Parked {
                    run_id: expected,
                    readiness: ParkedReadiness::Ready,
                },
                WorkerPermit::ResumeParked { .. },
            ) if *expected == run_id => {
                self.state = WorkerState::Running { run_id };
            }
            (state, _) => {
                panic!(
                    "invalid permit {:?} for worker {} in state {:?}",
                    permit.kind_name(),
                    worker.0,
                    state
                );
            }
        }

        let mut slot = lock_unpoison(&self.control.slot);
        assert!(
            slot.permit.is_none() && slot.report.is_none(),
            "sim worker {} rendezvous slot was not clean before grant",
            worker.0
        );
        slot.permit = Some(permit);
        self.control.cv.notify_one();
    }

    fn process_report(&mut self, report: WorkerReport) -> Option<ParkedStack> {
        match self.state {
            WorkerState::Running { run_id } if run_id == report.run_id => {}
            ref state => panic!(
                "stale report {:?} for worker {} run {:?} while in state {:?}",
                report.kind, report.worker.0, report.run_id, state
            ),
        }

        match report.kind {
            WorkerReportKind::PollReturned | WorkerReportKind::FinishedBlocking => {
                self.state = WorkerState::Idle;
                None
            }
            WorkerReportKind::Yielded => {
                self.state = WorkerState::Parked {
                    run_id: report.run_id,
                    readiness: ParkedReadiness::Ready,
                };
                Some(ParkedStack {
                    worker: report.worker,
                    run_id: report.run_id,
                })
            }
            WorkerReportKind::Blocked(reason) => {
                self.state = WorkerState::Parked {
                    run_id: report.run_id,
                    readiness: ParkedReadiness::Blocked(reason),
                };
                None
            }
            WorkerReportKind::Panicked => {
                self.state = WorkerState::Panicked { run_id: report.run_id };
                panic!(
                    "sim worker {} panicked while running scheduled work for run {:?}",
                    report.worker.0, report.run_id
                );
            }
        }
    }

    fn mark_mutex_ready(&mut self, waiter: MutexWaiter, mutex: MutexId) {
        match self.state {
            WorkerState::Parked {
                run_id,
                readiness: ParkedReadiness::Blocked(BlockReason::Mutex(blocked_on)),
            } if run_id == waiter.run_id && blocked_on == mutex => {
                self.state = WorkerState::Parked {
                    run_id,
                    readiness: ParkedReadiness::Ready,
                };
            }
            ref state => panic!(
                "mutex {:?} attempted to wake worker {} run {:?} in state {:?}",
                mutex, waiter.worker.0, waiter.run_id, state
            ),
        }
    }

    fn shutdown_if_idle(&mut self, worker: SimWorkerId) -> Option<ThreadJoinHandle<()>> {
        match self.state {
            WorkerState::Idle | WorkerState::Panicked { .. } => {
                let mut slot = lock_unpoison(&self.control.slot);
                if slot.permit.is_none() && slot.report.is_none() {
                    slot.permit = Some(WorkerPermit::Shutdown);
                    self.control.cv.notify_one();
                    self.join.take()
                } else {
                    eprintln!("sim worker {} had dirty rendezvous slot during executor drop", worker.0);
                    None
                }
            }
            WorkerState::Running { .. } | WorkerState::Parked { .. } => {
                // A stackful worker may still have a live Rust stack. Do not
                // block `Drop` trying to unwind it here.
                None
            }
        }
    }
}

/// Shared transport between the deterministic driver and worker threads.
///
/// This struct deliberately transports only permits and reports. It does not
/// make scheduling decisions; those remain in [`Executor::run_all_ready`] and
/// use the seeded runtime RNG.
struct SchedulerShared {
    workers: StdMutex<BTreeMap<SimWorkerId, WorkerRecord>>,
}

impl SchedulerShared {
    fn new() -> Self {
        Self {
            workers: StdMutex::new(BTreeMap::new()),
        }
    }
}

fn lock_unpoison<T>(mutex: &StdMutex<T>) -> StdMutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn send_worker_report(shared: &SchedulerShared, report: WorkerReport) {
    let control = {
        let workers = lock_unpoison(&shared.workers);
        workers
            .get(&report.worker)
            .unwrap_or_else(|| panic!("unknown simulated worker {}", report.worker.0))
            .control
            .clone()
    };
    let mut slot = lock_unpoison(&control.slot);
    assert!(
        slot.report.is_none(),
        "sim worker {} already has a pending report",
        report.worker.0
    );
    slot.report = Some(report);
    control.cv.notify_one();
}

/// Thread-local identity installed while a sim worker is running user code.
///
/// Synchronous APIs like [`yield_sync`] and [`SimMutex::lock`] use this to find
/// the current worker, report to the driver, and park the correct OS thread.
#[derive(Clone)]
struct CurrentWorker {
    id: SimWorkerId,
    shared: Arc<SchedulerShared>,
    control: Arc<WorkerControl>,
    sender: Sender,
    active_run_id: Arc<StdMutex<Option<RunId>>>,
}

thread_local! {
    static CURRENT_WORKER: RefCell<Option<CurrentWorker>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub seed: u64,
    pub max_sim_threads: usize,
}

impl RuntimeConfig {
    pub const fn new(seed: u64) -> Self {
        Self {
            seed,
            max_sim_threads: 64,
        }
    }

    pub const fn with_max_sim_threads(mut self, max_sim_threads: usize) -> Self {
        self.max_sim_threads = max_sim_threads;
        self
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new(0)
    }
}

/// A unique identifier for a simulated node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u64);

impl NodeId {
    /// The default node for single-node simulation or top-level runtime work.
    pub const MAIN: Self = Self(0);
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Immutable metadata attached to one simulated node.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct NodeConfig {
    name: Option<alloc::string::String>,
}

/// Builder for configuring a simulated node before it is created.
pub struct NodeBuilder {
    handle: Handle,
    config: NodeConfig,
}

impl NodeBuilder {
    /// Assign a human-readable name to the node.
    pub fn name(mut self, name: impl Into<alloc::string::String>) -> Self {
        self.config.name = Some(name.into());
        self
    }

    /// Create the node with the accumulated configuration.
    pub fn build(self) -> Node {
        self.handle.build_node(self.config)
    }
}

/// Handle to one simulated node in the runtime.
#[derive(Clone)]
pub struct Node {
    id: NodeId,
    handle: Handle,
    config: Arc<NodeConfig>,
}

impl Node {
    /// Return the stable identifier for this simulated node.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Return the optional human-readable name for this node.
    pub fn name(&self) -> Option<&str> {
        self.config.name.as_deref()
    }

    /// Pause scheduling for this node.
    pub fn pause(&self) {
        self.handle.pause(self.id);
    }

    /// Resume scheduling for this node.
    pub fn resume(&self) {
        self.handle.resume(self.id);
    }

    /// Spawn a `Send` future onto this simulated node.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle.spawn_on(self.id, future)
    }

    /// Spawn a non-`Send` future onto this simulated node.
    pub fn spawn_local<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.handle.spawn_local_on(self.id, future)
    }
}

/// A small single-threaded runtime for DST's top-level future.
///
/// futures are scheduled as runnables, the ready queue
/// is sampled by deterministic RNG, and pending execution without future events
/// is considered a test hang.
pub struct Runtime {
    executor: Arc<Executor>,
}

impl Runtime {
    /// Create a simulation runtime seeded for deterministic scheduling and RNG.
    pub fn new(seed: u64) -> Self {
        Self::with_config(RuntimeConfig::new(seed))
    }

    /// Create a simulation runtime from an explicit runtime configuration.
    pub fn with_config(config: RuntimeConfig) -> Self {
        Self {
            executor: Arc::new(Executor::new(config)),
        }
    }

    /// Drive a top-level future to completion on the simulation executor.
    ///
    /// While the future runs, spawned tasks share the same deterministic
    /// scheduler, timer wheel, and runtime RNG.
    pub fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        self.executor.block_on(future)
    }

    /// Return the amount of virtual time elapsed in this runtime.
    pub fn elapsed(&self) -> Duration {
        self.executor.elapsed()
    }

    /// Get a cloneable handle for spawning tasks and accessing runtime services.
    pub fn handle(&self) -> Handle {
        Handle {
            executor: Arc::clone(&self.executor),
        }
    }

    /// Create a new simulated node.
    ///
    /// Nodes are a scheduling/pausing boundary rather than separate executors:
    /// all nodes still run on the same single-threaded runtime.
    pub fn create_node(&self) -> NodeBuilder {
        self.handle().create_node()
    }

    /// Pause scheduling for a node.
    ///
    /// Tasks already queued for the node are retained and will run only after
    /// the node is resumed.
    pub fn pause(&self, node: NodeId) {
        self.handle().pause(node);
    }

    /// Resume scheduling for a previously paused node.
    pub fn resume(&self, node: NodeId) {
        self.handle().resume(node);
    }

    /// Spawn a `Send` future onto a specific simulated node.
    pub fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle().spawn_on(node, future)
    }

    pub fn enable_buggify(&self) {
        self.executor.enable_buggify();
    }

    /// Disable probabilistic fault injection for this runtime.
    pub fn disable_buggify(&self) {
        self.executor.disable_buggify();
    }

    /// Return whether buggify is enabled for this runtime.
    pub fn is_buggify_enabled(&self) -> bool {
        self.executor.is_buggify_enabled()
    }

    /// Sample the default runtime buggify probability.
    pub fn buggify(&self) -> bool {
        self.executor.buggify()
    }

    /// Sample a caller-provided runtime buggify probability.
    pub fn buggify_with_prob(&self, probability: f64) -> bool {
        self.executor.buggify_with_prob(probability)
    }

    #[allow(dead_code)]
    pub(crate) fn enable_determinism_log(&self) {
        self.executor.rng.enable_determinism_log();
    }

    #[allow(dead_code)]
    pub(crate) fn enable_determinism_check(&self, log: crate::sim::DeterminismLog) {
        self.executor.rng.enable_determinism_check(log);
    }

    #[allow(dead_code)]
    pub(crate) fn take_determinism_log(&self) -> Option<crate::sim::DeterminismLog> {
        self.executor.rng.take_determinism_log()
    }

    #[allow(dead_code)]
    pub(crate) fn finish_determinism_check(&self) -> Result<(), alloc::string::String> {
        self.executor.rng.finish_determinism_check()
    }
}

/// Cloneable access to the simulation executor.
#[derive(Clone)]
pub struct Handle {
    executor: Arc<Executor>,
}

impl Handle {
    /// Create a new simulated node owned by this runtime.
    pub fn create_node(&self) -> NodeBuilder {
        NodeBuilder {
            handle: self.clone(),
            config: NodeConfig::default(),
        }
    }

    fn build_node(&self, config: NodeConfig) -> Node {
        let id = self.executor.create_node(config.clone());
        let config = self.executor.node_config(id);
        Node {
            id,
            handle: self.clone(),
            config,
        }
    }

    /// Pause scheduling for a node.
    pub fn pause(&self, node: NodeId) {
        self.executor.pause(node);
    }

    /// Resume scheduling for a node and requeue any buffered tasks for it.
    pub fn resume(&self, node: NodeId) {
        self.executor.resume(node);
    }

    /// Spawn a `Send` future onto a specific simulated node.
    pub fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.executor.spawn_on(node, future)
    }

    /// Spawn a non-`Send` future onto a specific simulated node.
    ///
    /// This is only valid because the simulation executor is single-threaded.
    pub fn spawn_local_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.executor.spawn_local_on(node, future)
    }

    /// Return the current virtual time for this runtime.
    pub fn now(&self) -> Duration {
        self.executor.time.now()
    }

    /// Move virtual time forward explicitly.
    pub fn advance(&self, duration: Duration) {
        self.executor.time.advance(duration);
    }

    /// Create a future that becomes ready after `duration` of virtual time.
    pub fn sleep(&self, duration: Duration) -> crate::sim::time::Sleep {
        self.executor.time.sleep(duration)
    }

    /// Race a future against a virtual-time timeout.
    pub async fn timeout<T>(
        &self,
        duration: Duration,
        future: impl Future<Output = T>,
    ) -> Result<T, crate::sim::time::TimeoutElapsed> {
        self.executor.time.timeout(duration, future).await
    }

    /// Run a synchronous closure on a stackful simulated worker.
    ///
    /// The closure may call [`yield_sync`] or block on [`SimMutex`]. Unlike
    /// Tokio's blocking pool, the simulator still allows only one worker to
    /// execute user code at a time; extra workers exist only to preserve parked
    /// synchronous stacks.
    pub async fn spawn_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.executor.spawn_blocking(f).await
    }

    pub fn enable_buggify(&self) {
        self.executor.enable_buggify();
    }

    /// Disable probabilistic fault injection for this runtime.
    pub fn disable_buggify(&self) {
        self.executor.disable_buggify();
    }

    /// Return whether buggify is enabled for this runtime.
    pub fn is_buggify_enabled(&self) -> bool {
        self.executor.is_buggify_enabled()
    }

    /// Sample the default runtime buggify probability.
    pub fn buggify(&self) -> bool {
        self.executor.buggify()
    }

    /// Sample a caller-provided runtime buggify probability.
    pub fn buggify_with_prob(&self, probability: f64) -> bool {
        self.executor.buggify_with_prob(probability)
    }
}

/// Core single-threaded scheduler backing a simulation [`Runtime`].
///
/// The executor owns the runnable queue, per-node pause state, deterministic
/// RNG, and virtual time. Tasks are selected from the queue using the runtime
/// RNG so the schedule is reproducible for a given seed.
struct Executor {
    queue: Receiver,
    sender: Sender,
    nodes: spin::Mutex<BTreeMap<NodeId, Arc<NodeRecord>>>,
    next_node: AtomicU64,
    next_worker: AtomicU64,
    next_run: AtomicU64,
    max_sim_threads: usize,
    shared: Arc<SchedulerShared>,
    rng: Rng,
    time: TimeHandle,
}

impl Executor {
    /// Construct a fresh executor with one default `MAIN` node.
    fn new(config: RuntimeConfig) -> Self {
        let queue = Queue::new();
        let mut nodes = BTreeMap::new();
        nodes.insert(NodeId::MAIN, Arc::new(NodeRecord::default()));
        Self {
            queue: queue.receiver(),
            sender: queue.sender(),
            nodes: spin::Mutex::new(nodes),
            next_node: AtomicU64::new(1),
            next_worker: AtomicU64::new(0),
            next_run: AtomicU64::new(0),
            max_sim_threads: config.max_sim_threads,
            shared: Arc::new(SchedulerShared::new()),
            rng: Rng::new(config.seed),
            time: TimeHandle::new(),
        }
    }

    fn elapsed(&self) -> Duration {
        self.time.now()
    }

    fn enable_buggify(&self) {
        self.rng.enable_buggify();
    }

    fn disable_buggify(&self) {
        self.rng.disable_buggify();
    }

    fn is_buggify_enabled(&self) -> bool {
        self.rng.is_buggify_enabled()
    }

    fn buggify(&self) -> bool {
        self.rng.buggify()
    }

    fn buggify_with_prob(&self, probability: f64) -> bool {
        self.rng.buggify_with_prob(probability)
    }

    fn create_node(&self, config: NodeConfig) -> NodeId {
        let id = NodeId(self.next_node.fetch_add(1, Ordering::Relaxed));
        self.nodes.lock().insert(
            id,
            Arc::new(NodeRecord {
                config: Arc::new(config),
                state: NodeState::default(),
            }),
        );
        id
    }

    fn node_config(&self, node: NodeId) -> Arc<NodeConfig> {
        self.node_record(node).config.clone()
    }

    /// Mark a node as paused so newly selected runnables are buffered.
    fn pause(&self, node: NodeId) {
        self.node_record(node).state.paused.store(true, Ordering::Relaxed);
    }

    /// Mark a node as runnable again and requeue any buffered tasks for it.
    fn resume(&self, node: NodeId) {
        let record = self.node_record(node);
        record.state.paused.store(false, Ordering::Relaxed);

        let mut paused = record.state.paused_queue.lock();
        for runnable in paused.drain(..) {
            self.sender.send(runnable);
        }
    }

    /// Spawn a `Send` task and enqueue its runnable on the shared runtime queue.
    fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.assert_known_node(node);

        let abort = AbortHandle::new();
        let abortable = Abortable::new(future, abort.clone());
        let sender = self.sender.clone();
        let (runnable, task) = async_task::Builder::new().metadata(node).spawn(
            move |_| abortable,
            move |runnable| sender.send(RunnableEntity::Async(runnable)),
        );
        runnable.schedule();

        JoinHandle { task, abort }
    }

    /// Spawn a non-`Send` task on the single-threaded runtime.
    fn spawn_local_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.assert_known_node(node);

        let abort = AbortHandle::new();
        let abortable = Abortable::new(future, abort.clone());
        let sender = self.sender.clone();
        let (runnable, task) = unsafe {
            async_task::Builder::new().metadata(node).spawn_unchecked(
                move |_| abortable,
                move |runnable| sender.send(RunnableEntity::LocalAsync(runnable)),
            )
        };
        runnable.schedule();

        JoinHandle { task, abort }
    }

    #[track_caller]
    /// Run the top-level future until completion.
    ///
    /// The executor repeatedly drains runnable tasks, then advances virtual
    /// time to the next timer when the queue is empty. If neither runnable work
    /// nor timers remain, the simulation is considered deadlocked.
    fn block_on<F: Future>(&self, future: F) -> F::Output {
        let sender = self.sender.clone();
        let (runnable, mut task) = unsafe {
            async_task::Builder::new().metadata(NodeId::MAIN).spawn_unchecked(
                move |_| future,
                move |runnable| sender.send(RunnableEntity::LocalAsync(runnable)),
            )
        };
        runnable.schedule();

        loop {
            self.run_all_ready();
            if task.is_finished() {
                let waker = Waker::noop();
                return match Pin::new(&mut task).poll(&mut Context::from_waker(waker)) {
                    Poll::Ready(output) => output,
                    Poll::Pending => unreachable!("task.is_finished() was true"),
                };
            }

            if self.time.wake_next_timer() {
                continue;
            }

            panic!("{}", self.deadlock_diagnostic());
        }
    }

    fn deadlock_diagnostic(&self) -> String {
        let queue_len = self.queue.len();
        let workers = lock_unpoison(&self.shared.workers);
        let idle_workers = workers
            .values()
            .filter(|record| matches!(record.state, WorkerState::Idle))
            .count();
        let yielded_workers = workers
            .values()
            .filter(|record| {
                matches!(
                    record.state,
                    WorkerState::Parked {
                        readiness: ParkedReadiness::Ready,
                        ..
                    }
                )
            })
            .count();
        let blocked_workers = workers
            .values()
            .filter(|record| {
                matches!(
                    record.state,
                    WorkerState::Parked {
                        readiness: ParkedReadiness::Blocked(_),
                        ..
                    }
                )
            })
            .count();
        let worker_states = workers
            .iter()
            .map(|(id, record)| format!("worker {}: {}", id.0, record.state.describe()))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "no runnable tasks; all simulated tasks are blocked; queue_len={queue_len}, idle_workers={idle_workers}, yielded_workers={yielded_workers}, blocked_workers={blocked_workers}, workers=[{worker_states}]"
        )
    }

    /// Drain the runnable queue, selecting tasks in deterministic RNG order.
    ///
    /// Paused-node tasks are diverted into that node's paused buffer instead of
    /// being polled immediately.
    fn run_all_ready(&self) {
        while let Some(entity) = self.queue.try_recv_random(&self.rng) {
            match entity {
                entity @ (RunnableEntity::Async(_) | RunnableEntity::LocalAsync(_)) => {
                    let runnable = match &entity {
                        RunnableEntity::Async(runnable) | RunnableEntity::LocalAsync(runnable) => runnable,
                        _ => unreachable!(),
                    };
                    let node = *runnable.metadata();
                    let record = self.node_record(node);
                    if record.state.paused.load(Ordering::Relaxed) {
                        record.state.paused_queue.lock().push(entity);
                        continue;
                    }
                    self.run_entity(entity);
                }
                RunnableEntity::Parked(stack) => self.run_entity(RunnableEntity::Parked(stack)),
                RunnableEntity::Blocking(job) => self.run_entity(RunnableEntity::Blocking(job)),
            }
        }
    }

    fn spawn_blocking<F, R>(&self, f: F) -> BlockingJoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let state = Arc::new(BlockingState::<R> {
            output: StdMutex::new(None),
            waker: StdMutex::new(None),
        });
        let state_for_job = state.clone();
        self.sender.send(RunnableEntity::Blocking(Box::new(move || {
            let output = catch_unwind(AssertUnwindSafe(f));
            *lock_unpoison(&state_for_job.output) = Some(output);
            if let Some(waker) = lock_unpoison(&state_for_job.waker).take() {
                waker.wake();
            }
        })));
        BlockingJoinHandle { state }
    }

    fn run_entity(&self, entity: RunnableEntity) {
        match entity {
            RunnableEntity::Parked(stack) => {
                // The worker is already holding a synchronous Rust stack. We
                // only grant permission for that same stack to continue.
                self.grant(stack.worker, WorkerPermit::ResumeParked { run_id: stack.run_id });
                self.process_report(self.wait_for_report(stack.worker, stack.run_id));
            }
            RunnableEntity::Async(runnable) => {
                // A `Send` async runnable does not need a dedicated stack after
                // it returns `Pending`, so any idle worker can poll it once.
                let worker = self.acquire_worker();
                let run_id = self.next_run_id();
                self.grant(worker, WorkerPermit::RunAsync { run_id, runnable });
                self.process_report(self.wait_for_report(worker, run_id));
            }
            RunnableEntity::LocalAsync(runnable) => {
                // Non-`Send` futures cannot move to an OS worker. Keep the old
                // local execution path for `spawn_local` and top-level
                // `block_on` compatibility.
                runnable.run();
            }
            RunnableEntity::Blocking(job) => {
                // Blocking jobs are stackful and may park, so they need a sim
                // worker rather than direct execution on the driver.
                let worker = self.acquire_worker();
                let run_id = self.next_run_id();
                self.grant(worker, WorkerPermit::RunBlocking { run_id, job });
                self.process_report(self.wait_for_report(worker, run_id));
            }
        }
        self.assert_invariants();
    }

    fn next_run_id(&self) -> RunId {
        RunId(self.next_run.fetch_add(1, Ordering::Relaxed))
    }

    fn acquire_worker(&self) -> SimWorkerId {
        {
            let workers = lock_unpoison(&self.shared.workers);
            if let Some((worker, _)) = workers
                .iter()
                .find(|(_, record)| matches!(record.state, WorkerState::Idle))
            {
                return *worker;
            }
        }
        self.create_worker()
    }

    fn create_worker(&self) -> SimWorkerId {
        let raw = self.next_worker.fetch_add(1, Ordering::Relaxed);
        if raw as usize >= self.max_sim_threads {
            panic!("sim worker limit exhausted: max_sim_threads={}", self.max_sim_threads);
        }
        let id = SimWorkerId(raw);
        let control = Arc::new(WorkerControl::new());
        let active_run_id = Arc::new(StdMutex::new(None));
        let shared = self.shared.clone();
        let sender = self.sender.clone();
        let control_for_thread = control.clone();
        let active_run_for_thread = active_run_id.clone();
        let join = crate::sim_std::allow_sim_thread_spawn(|| {
            thread::Builder::new()
                .name(format!("spacetimedb-sim-worker-{}", id.0))
                .spawn(move || worker_main(id, shared, control_for_thread, sender, active_run_for_thread))
                .expect("failed to spawn simulated worker thread")
        });
        lock_unpoison(&self.shared.workers).insert(
            id,
            WorkerRecord {
                control,
                state: WorkerState::Idle,
                join: Some(join),
            },
        );
        id
    }

    fn grant(&self, worker: SimWorkerId, permit: WorkerPermit) {
        let mut workers = lock_unpoison(&self.shared.workers);
        let record = workers
            .get_mut(&worker)
            .unwrap_or_else(|| panic!("unknown simulated worker {}", worker.0));
        record.grant(worker, permit);
    }

    /// Wait until `worker` reports back after consuming its current permit.
    ///
    /// This is what enforces the "only one worker executes user code" rule:
    /// the driver does not grant any other permit while it is waiting here.
    fn wait_for_report(&self, worker: SimWorkerId, run_id: RunId) -> WorkerReport {
        let control = {
            let workers = lock_unpoison(&self.shared.workers);
            workers
                .get(&worker)
                .unwrap_or_else(|| panic!("unknown simulated worker {}", worker.0))
                .control
                .clone()
        };
        let mut slot = lock_unpoison(&control.slot);
        loop {
            if let Some(report) = slot.report.take() {
                if report.worker == worker && report.run_id == run_id {
                    return report;
                }
                panic!(
                    "unexpected sim worker report {:?}; driver was waiting for worker {} run {:?}",
                    report, worker.0, run_id
                );
            }
            slot = control.cv.wait(slot).unwrap_or_else(|poison| poison.into_inner());
        }
    }

    fn process_report(&self, report: WorkerReport) {
        let mut workers = lock_unpoison(&self.shared.workers);
        let record = workers
            .get_mut(&report.worker)
            .unwrap_or_else(|| panic!("unknown simulated worker {}", report.worker.0));
        if let Some(stack) = record.process_report(report) {
            drop(workers);
            self.sender.send(RunnableEntity::Parked(stack));
        }
    }

    fn assert_invariants(&self) {
        #[cfg(any(test, debug_assertions))]
        {
            let workers = lock_unpoison(&self.shared.workers);
            let running = workers
                .values()
                .filter(|record| matches!(record.state, WorkerState::Running { .. }))
                .count();
            assert!(running <= 1, "more than one simulated worker is running: {running}");

            for (worker, record) in workers.iter() {
                let slot = lock_unpoison(&record.control.slot);
                assert!(
                    !(slot.permit.is_some() && slot.report.is_some()),
                    "sim worker {} has both a pending permit and report",
                    worker.0
                );
                if matches!(record.state, WorkerState::Idle) {
                    assert!(
                        slot.permit.is_none(),
                        "idle sim worker {} has a pending permit",
                        worker.0
                    );
                }
            }

            for stack in self.queue.parked_stacks() {
                let record = workers
                    .get(&stack.worker)
                    .unwrap_or_else(|| panic!("ready queue contains unknown parked worker {}", stack.worker.0));
                assert!(
                    matches!(
                        record.state,
                        WorkerState::Parked {
                            run_id,
                            readiness: ParkedReadiness::Ready,
                        } if run_id == stack.run_id
                    ),
                    "ready queue contains stale parked stack {:?} for worker state {:?}",
                    stack,
                    record.state
                );
            }
        }
    }

    /// Look up the record for a node, panicking if the node is unknown.
    fn node_record(&self, node: NodeId) -> Arc<NodeRecord> {
        self.nodes
            .lock()
            .get(&node)
            .cloned()
            .unwrap_or_else(|| panic!("unknown simulated node {node}"))
    }

    fn assert_known_node(&self, node: NodeId) {
        let _ = self.node_record(node);
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        let mut joins = Vec::new();
        {
            let mut workers = lock_unpoison(&self.shared.workers);
            for (id, record) in workers.iter_mut() {
                if let Some(join) = record.shutdown_if_idle(*id) {
                    joins.push(join);
                }
            }
        }

        for join in joins {
            let _ = join.join();
        }
    }
}

/// One simulated node's immutable metadata plus scheduler state.
#[derive(Clone, Default)]
struct NodeRecord {
    config: Arc<NodeConfig>,
    state: NodeState,
}

/// Per-node scheduler state shared by tasks assigned to that node.
#[derive(Clone, Default)]
struct NodeState {
    paused: Arc<AtomicBool>,
    paused_queue: Arc<Mutex<Vec<RunnableEntity>>>,
}

struct BlockingState<T> {
    output: StdMutex<Option<thread::Result<T>>>,
    waker: StdMutex<Option<Waker>>,
}

pub struct BlockingJoinHandle<T> {
    state: Arc<BlockingState<T>>,
}

impl<T> Future for BlockingJoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(output) = lock_unpoison(&self.state.output).take() {
            return match output {
                Ok(value) => Poll::Ready(value),
                Err(payload) => std::panic::resume_unwind(payload),
            };
        }
        lock_unpoison(&self.state.waker).replace(cx.waker().clone());
        Poll::Pending
    }
}

/// Yield back to the scheduler once.
///
/// This is the smallest explicit interleaving point available to simulated
/// tasks when they need to give other runnables a chance to execute.
pub async fn yield_now() {
    YieldNow { yielded: false }.await
}

/// Yield from synchronous code running on a simulated worker.
///
/// This is the stackful counterpart to [`yield_now`]. The current worker first
/// reports `Yielded` to the deterministic driver, then parks on its worker
/// condvar. When the driver later schedules the corresponding
/// [`RunnableEntity::Parked`], execution resumes on the same OS thread
/// and returns from this function.
///
/// Panics when called outside a simulated worker, because ordinary synchronous
/// Rust has no stackful suspension point for the simulator to resume.
pub fn yield_sync() {
    let worker = current_worker_or_panic("sim::yield_sync");
    let run_id = worker_active_run_id(&worker);
    send_worker_report(
        &worker.shared,
        WorkerReport {
            worker: worker.id,
            run_id,
            kind: WorkerReportKind::Yielded,
        },
    );
    park_current_worker(&worker, run_id);
}

/// Blocking mutex whose wait state is visible to the simulation scheduler.
///
/// This must be used instead of `std::sync::Mutex` when synchronous simulated
/// code may contend on a lock. Contention reports `Blocked(Mutex(_))` to the
/// driver and parks the current worker; the worker is not runnable again until
/// the guard owner drops and hands ownership to a waiter.
pub struct SimMutex<T> {
    id: MutexId,
    state: StdMutex<SimMutexState>,
    value: UnsafeCell<T>,
}

/// Scheduler-visible mutex metadata.
///
/// The short-lived `StdMutex` around this state protects metadata only. It is
/// never used as the blocking mechanism for contended simulated locks.
struct SimMutexState {
    owner: Option<SimWorkerId>,
    waiters: VecDeque<MutexWaiter>,
}

/// Guard returned by [`SimMutex::lock`] and [`SimMutex::try_lock`].
///
/// Dropping the guard either unlocks the mutex or transfers ownership directly
/// to the next parked worker and requeues that worker for deterministic
/// scheduling.
pub struct SimMutexGuard<'a, T> {
    mutex: &'a SimMutex<T>,
    _not_send: PhantomData<Rc<()>>,
}

unsafe impl<T: Send> Send for SimMutex<T> {}
unsafe impl<T: Send> Sync for SimMutex<T> {}

static NEXT_MUTEX_ID: AtomicU64 = AtomicU64::new(1);

impl<T> SimMutex<T> {
    /// Create a new scheduler-visible mutex.
    pub fn new(value: T) -> Self {
        Self {
            id: MutexId(NEXT_MUTEX_ID.fetch_add(1, Ordering::Relaxed)),
            state: StdMutex::new(SimMutexState {
                owner: None,
                waiters: VecDeque::new(),
            }),
            value: UnsafeCell::new(value),
        }
    }

    /// Try to acquire the mutex without parking the worker.
    ///
    /// Panics outside a simulated worker so that all successful ownership is
    /// associated with a scheduler-visible worker id.
    pub fn try_lock(&self) -> Option<SimMutexGuard<'_, T>> {
        let worker = current_worker_or_panic("SimMutex::try_lock");
        let mut state = lock_unpoison(&self.state);
        if state.owner.is_none() {
            state.owner = Some(worker.id);
            Some(SimMutexGuard {
                mutex: self,
                _not_send: PhantomData,
            })
        } else {
            None
        }
    }

    /// Acquire the mutex, parking the current worker if it is contended.
    ///
    /// If another worker owns the lock, the current worker is added to the
    /// waiter list, reports `Blocked(Mutex(_))`, and parks. It resumes only
    /// after guard drop transfers ownership to it.
    pub fn lock(&self) -> SimMutexGuard<'_, T> {
        let worker = current_worker_or_panic("SimMutex::lock");
        loop {
            {
                let mut state = lock_unpoison(&self.state);
                if state.owner.is_none() {
                    state.owner = Some(worker.id);
                    return SimMutexGuard {
                        mutex: self,
                        _not_send: PhantomData,
                    };
                }
                if state.owner == Some(worker.id) {
                    panic!("SimMutex::lock called recursively by worker {}", worker.id.0);
                }
                let run_id = worker_active_run_id(&worker);
                if !state.waiters.iter().any(|waiter| waiter.worker == worker.id) {
                    state.waiters.push_back(MutexWaiter {
                        worker: worker.id,
                        run_id,
                    });
                }
                drop(state);

                send_worker_report(
                    &worker.shared,
                    WorkerReport {
                        worker: worker.id,
                        run_id,
                        kind: WorkerReportKind::Blocked(BlockReason::Mutex(self.id)),
                    },
                );
                park_current_worker(&worker, run_id);

                let state = lock_unpoison(&self.state);
                if state.owner == Some(worker.id) {
                    return SimMutexGuard {
                        mutex: self,
                        _not_send: PhantomData,
                    };
                }
            }
        }
    }
}

impl<T> Deref for SimMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for SimMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Drop for SimMutexGuard<'_, T> {
    fn drop(&mut self) {
        let worker = current_worker_or_panic("SimMutexGuard::drop");
        let next = {
            let mut state = lock_unpoison(&self.mutex.state);
            assert_eq!(
                state.owner,
                Some(worker.id),
                "SimMutexGuard dropped by a worker that does not own the mutex"
            );
            if let Some(next) = state.waiters.pop_front() {
                state.owner = Some(next.worker);
                Some(next)
            } else {
                state.owner = None;
                None
            }
        };

        if let Some(next) = next {
            wake_worker_blocked_on_mutex(&worker.shared, next, self.mutex.id);
            // Ownership has already been transferred in mutex metadata. The
            // waiter can now be scheduled and return from its blocked `lock`.
            worker.sender.send(RunnableEntity::Parked(ParkedStack {
                worker: next.worker,
                run_id: next.run_id,
            }));
        }
    }
}

fn wake_worker_blocked_on_mutex(shared: &SchedulerShared, waiter: MutexWaiter, mutex: MutexId) {
    let mut workers = lock_unpoison(&shared.workers);
    let record = workers
        .get_mut(&waiter.worker)
        .unwrap_or_else(|| panic!("mutex attempted to wake unknown worker {}", waiter.worker.0));
    record.mark_mutex_ready(waiter, mutex);
}

fn current_worker_or_panic(api: &str) -> CurrentWorker {
    CURRENT_WORKER.with(|worker| {
        worker
            .borrow()
            .clone()
            .unwrap_or_else(|| panic!("{api} called outside a simulated worker"))
    })
}

fn worker_active_run_id(worker: &CurrentWorker) -> RunId {
    worker
        .active_run_id
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .unwrap_or_else(|| panic!("sim worker {} has no active run id", worker.id.0))
}

/// One-shot future backing [`yield_now`].
struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn worker_main(
    id: SimWorkerId,
    shared: Arc<SchedulerShared>,
    control: Arc<WorkerControl>,
    sender: Sender,
    active_run_id: Arc<StdMutex<Option<RunId>>>,
) {
    let _simulation_thread = crate::sim_std::enter_simulation_thread();
    CURRENT_WORKER.with(|worker| {
        worker.replace(Some(CurrentWorker {
            id,
            shared: shared.clone(),
            control: control.clone(),
            sender,
            active_run_id: active_run_id.clone(),
        }));
    });

    loop {
        let permit = wait_for_permit(&control);
        match permit {
            WorkerPermit::RunAsync { run_id, runnable } => {
                *lock_unpoison(&active_run_id) = Some(run_id);
                // Poll exactly one async runnable. If it returns `Pending`, its
                // stack has unwound into the future state machine and this
                // worker is reusable.
                let result = catch_unwind(AssertUnwindSafe(|| runnable.run()));
                *lock_unpoison(&active_run_id) = None;
                send_worker_report(
                    &shared,
                    WorkerReport {
                        worker: id,
                        run_id,
                        kind: if result.is_ok() {
                            WorkerReportKind::PollReturned
                        } else {
                            WorkerReportKind::Panicked
                        },
                    },
                );
            }
            WorkerPermit::RunBlocking { run_id, job } => {
                *lock_unpoison(&active_run_id) = Some(run_id);
                // Run a stackful closure. The closure may temporarily park this
                // worker via `yield_sync` or `SimMutex::lock`; once it returns,
                // the worker can be reused.
                let result = catch_unwind(AssertUnwindSafe(|| job()));
                *lock_unpoison(&active_run_id) = None;
                send_worker_report(
                    &shared,
                    WorkerReport {
                        worker: id,
                        run_id,
                        kind: if result.is_ok() {
                            WorkerReportKind::FinishedBlocking
                        } else {
                            WorkerReportKind::Panicked
                        },
                    },
                );
            }
            WorkerPermit::ResumeParked { run_id } => {
                assert_eq!(
                    *lock_unpoison(&active_run_id),
                    Some(run_id),
                    "resume permit run id did not match parked worker"
                );
            }
            WorkerPermit::Shutdown => break,
        }
    }
}

fn wait_for_permit(control: &WorkerControl) -> WorkerPermit {
    let mut slot = lock_unpoison(&control.slot);
    loop {
        if let Some(permit) = slot.permit.take() {
            return permit;
        }
        slot = control.cv.wait(slot).unwrap_or_else(|poison| poison.into_inner());
    }
}

fn park_current_worker(worker: &CurrentWorker, run_id: RunId) {
    loop {
        match wait_for_permit(&worker.control) {
            WorkerPermit::ResumeParked { run_id: resumed } if resumed == run_id => return,
            WorkerPermit::ResumeParked { run_id: resumed } => {
                panic!(
                    "sim worker {} resumed with wrong run id {:?}, expected {:?}",
                    worker.id.0, resumed, run_id
                );
            }
            WorkerPermit::Shutdown => panic!("sim worker shut down while parked"),
            WorkerPermit::RunAsync { .. } | WorkerPermit::RunBlocking { .. } => {
                panic!("sim worker received new work while parked")
            }
        }
    }
}

/// Shared runnable queue used by the simulation executor.
/// TODO: Make it generic over T
struct Queue {
    inner: Arc<QueueInner>,
}

/// Sending end of the runnable queue.
#[derive(Clone)]
struct Sender {
    inner: Arc<QueueInner>,
}

/// Receiving end of the runnable queue.
#[derive(Clone)]
struct Receiver {
    inner: Arc<QueueInner>,
}

/// Queue storage for runnables awaiting scheduling.
struct QueueInner {
    queue: Mutex<Vec<RunnableEntity>>,
}

impl Queue {
    fn new() -> Self {
        Self {
            inner: Arc::new(QueueInner {
                queue: Mutex::new(Vec::new()),
            }),
        }
    }

    fn sender(&self) -> Sender {
        Sender {
            inner: self.inner.clone(),
        }
    }

    fn receiver(&self) -> Receiver {
        Receiver {
            inner: self.inner.clone(),
        }
    }
}

impl Sender {
    /// Push a runnable onto the shared queue.
    fn send(&self, runnable: RunnableEntity) {
        self.inner.queue.lock().push(runnable);
    }
}

impl Receiver {
    /// Remove one runnable using the runtime RNG to choose among ready tasks.
    fn try_recv_random(&self, rng: &Rng) -> Option<RunnableEntity> {
        let mut queue = self.inner.queue.lock();
        if queue.is_empty() {
            return None;
        }
        let idx = rng.index(queue.len());
        Some(queue.swap_remove(idx))
    }

    fn len(&self) -> usize {
        self.inner.queue.lock().len()
    }

    #[cfg(any(test, debug_assertions))]
    fn parked_stacks(&self) -> Vec<ParkedStack> {
        self.inner
            .queue
            .lock()
            .iter()
            .filter_map(|entity| match entity {
                RunnableEntity::Parked(stack) => Some(*stack),
                RunnableEntity::Async(_) | RunnableEntity::LocalAsync(_) | RunnableEntity::Blocking(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex as StdMutex,
    };

    use super::*;
    use crate::sim::RuntimeConfig;

    #[test]
    fn paused_node_does_not_run_until_resumed() {
        let mut runtime = Runtime::new(1);
        let node = runtime.create_node().name("paused").build();
        node.pause();

        let runs = Arc::new(AtomicUsize::new(0));
        let task_runs = Arc::clone(&runs);
        let task = node.spawn(async move {
            task_runs.fetch_add(1, Ordering::SeqCst);
            7
        });

        runtime.block_on(async {
            yield_now().await;
        });
        assert_eq!(runs.load(Ordering::SeqCst), 0);

        node.resume();
        assert_eq!(runtime.block_on(task).expect("paused task should complete"), 7);
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn handle_can_spawn_onto_node_from_simulated_task() {
        let mut runtime = Runtime::new(2);
        let handle = runtime.handle();

        let value = runtime.block_on(async move {
            let node = handle.create_node().name("spawned").build();
            node.spawn(async { 11 }).await.expect("spawned task should complete")
        });

        assert_eq!(value, 11);
    }

    #[test]
    fn runtime_config_sets_seed() {
        let runtime = Runtime::with_config(RuntimeConfig::new(77));
        let handle = runtime.handle();
        handle.enable_buggify();

        let actual = (0..8).map(|_| handle.buggify_with_prob(0.5)).collect::<Vec<_>>();

        let expected = {
            let rng = Rng::new(77);
            rng.enable_buggify();
            (0..8).map(|_| rng.buggify_with_prob(0.5)).collect::<Vec<_>>()
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn runtime_and_handle_share_buggify_state() {
        let runtime = Runtime::new(6);
        let handle = runtime.handle();

        assert!(!runtime.is_buggify_enabled());
        runtime.enable_buggify();
        assert!(handle.is_buggify_enabled());
        assert!(handle.buggify_with_prob(1.0));
        handle.disable_buggify();
        assert!(!runtime.is_buggify_enabled());
    }

    #[test]
    fn aborted_task_returns_join_error_when_awaited() {
        let mut runtime = Runtime::new(8);
        let node = runtime.create_node().name("abort").build();
        let task = node.spawn(async move {
            yield_now().await;
            99
        });
        task.abort_handle().abort();

        let err = runtime
            .block_on(task)
            .expect_err("aborted task should surface JoinError instead of panicking");
        assert_eq!(err, JoinError);
    }

    #[cfg(feature = "simulation")]
    #[test]
    fn sim_std_block_on_can_spawn_local_task_with_explicit_handle() {
        let mut runtime = Runtime::new(5);
        let handle = runtime.handle();
        let node = handle.create_node().name("local").build();
        let value = crate::sim_std::block_on(&mut runtime, async move {
            let captured = std::rc::Rc::new(17);
            node.spawn_local(async move {
                yield_now().await;
                *captured
            })
            .await
            .expect("spawned local task should complete")
        });

        assert_eq!(value, 17);
    }

    #[test]
    fn node_builder_sets_name() {
        let runtime = Runtime::new(9);
        let unnamed = runtime.create_node().build();
        let named = runtime.create_node().name("replica-1").build();

        assert_eq!(unnamed.name(), None);
        assert_eq!(named.name(), Some("replica-1"));
        assert_ne!(unnamed.id(), named.id());
    }

    #[cfg(feature = "simulation")]
    #[test]
    fn check_determinism_runs_future_twice() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        CALLS.store(0, Ordering::SeqCst);

        let value = crate::sim_std::check_determinism(3, || async {
            CALLS.fetch_add(1, Ordering::SeqCst);
            yield_now().await;
            13
        });

        assert_eq!(value, 13);
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
    }

    #[cfg(feature = "simulation")]
    #[test]
    #[should_panic(expected = "non-determinism detected")]
    fn check_determinism_rejects_different_scheduler_sequence() {
        static FIRST_RUN: AtomicBool = AtomicBool::new(true);
        FIRST_RUN.store(true, Ordering::SeqCst);

        crate::sim_std::check_determinism(4, || async {
            if FIRST_RUN.swap(false, Ordering::SeqCst) {
                yield_now().await;
            }
        });
    }

    #[test]
    #[should_panic(expected = "sim::yield_sync called outside a simulated worker")]
    fn yield_sync_panics_outside_sim_worker() {
        yield_sync();
    }

    #[test]
    #[should_panic(expected = "SimMutex::lock called outside a simulated worker")]
    fn sim_mutex_lock_panics_outside_sim_worker() {
        let mutex = SimMutex::new(1);
        let _guard = mutex.lock();
    }

    #[test]
    fn spawn_blocking_can_yield_synchronously() {
        let mut runtime = Runtime::new(10);
        let handle = runtime.handle();
        let log = Arc::new(StdMutex::new(Vec::new()));

        runtime.block_on({
            let log = log.clone();
            async move {
                let first = handle.spawn_blocking({
                    let log = log.clone();
                    move || {
                        log.lock().unwrap().push("first: before yield");
                        yield_sync();
                        log.lock().unwrap().push("first: after yield");
                    }
                });

                let second = handle.spawn_blocking({
                    let log = log.clone();
                    move || {
                        log.lock().unwrap().push("second");
                    }
                });

                first.await;
                second.await;
            }
        });

        let log = log.lock().unwrap().clone();
        assert!(log.contains(&"first: before yield"));
        assert!(log.contains(&"first: after yield"));
        assert!(log.contains(&"second"));
        assert!(
            log.iter().position(|entry| *entry == "first: before yield")
                < log.iter().position(|entry| *entry == "first: after yield")
        );
    }

    #[test]
    fn spawn_blocking_surfaces_panic_to_awaiter() {
        let mut runtime = Runtime::new(11);
        let handle = runtime.handle();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.block_on(async move {
                handle
                    .spawn_blocking(|| -> () {
                        panic!("blocking failure");
                    })
                    .await;
            });
        }));

        assert!(result.is_err());
    }

    #[test]
    fn sim_mutex_blocks_and_hands_off_ownership() {
        let mut runtime = Runtime::new(12);
        let handle = runtime.handle();
        let mutex = Arc::new(SimMutex::new(0usize));
        let log = Arc::new(StdMutex::new(Vec::new()));

        runtime.block_on({
            let mutex = mutex.clone();
            let log = log.clone();
            async move {
                let first = handle.spawn_blocking({
                    let mutex = mutex.clone();
                    let log = log.clone();
                    move || {
                        let mut guard = mutex.lock();
                        log.lock().unwrap().push("first: locked");
                        *guard += 1;
                        yield_sync();
                        log.lock().unwrap().push("first: unlocking");
                    }
                });

                let second = handle.spawn_blocking({
                    let mutex = mutex.clone();
                    let log = log.clone();
                    move || {
                        let mut guard = mutex.lock();
                        log.lock().unwrap().push("second: locked");
                        *guard += 1;
                    }
                });

                first.await;
                second.await;
            }
        });

        let log = log.lock().unwrap().clone();
        assert!(log.contains(&"first: locked"));
        assert!(log.contains(&"first: unlocking"));
        assert!(log.contains(&"second: locked"));
    }

    #[test]
    fn sim_workers_do_not_run_user_code_concurrently() {
        let mut runtime = Runtime::new(13);
        let handle = runtime.handle();
        let running = Arc::new(AtomicUsize::new(0));

        runtime.block_on({
            let running = running.clone();
            async move {
                let mut joins = Vec::new();
                for _ in 0..4 {
                    let running = running.clone();
                    joins.push(handle.spawn_blocking(move || {
                        assert_eq!(running.fetch_add(1, Ordering::SeqCst), 0);
                        yield_sync();
                        assert_eq!(running.fetch_sub(1, Ordering::SeqCst), 1);
                    }));
                }
                for join in joins {
                    join.await;
                }
            }
        });
    }

    #[test]
    fn yielded_worker_can_resume_multiple_slices() {
        let mut runtime = Runtime::new(14);
        let handle = runtime.handle();
        let log = Arc::new(StdMutex::new(Vec::new()));

        runtime.block_on({
            let log = log.clone();
            async move {
                handle
                    .spawn_blocking({
                        let log = log.clone();
                        move || {
                            log.lock().unwrap().push("before first yield");
                            yield_sync();
                            log.lock().unwrap().push("before second yield");
                            yield_sync();
                            log.lock().unwrap().push("after second yield");
                        }
                    })
                    .await;
            }
        });

        assert_eq!(
            *log.lock().unwrap(),
            vec!["before first yield", "before second yield", "after second yield"]
        );
    }

    #[test]
    #[should_panic(expected = "unexpected sim worker report")]
    fn stale_worker_report_panics() {
        let executor = Executor::new(RuntimeConfig::new(15));
        let worker = executor.acquire_worker();
        let control = {
            let workers = lock_unpoison(&executor.shared.workers);
            workers.get(&worker).unwrap().control.clone()
        };
        lock_unpoison(&control.slot).report = Some(WorkerReport {
            worker,
            run_id: RunId(99),
            kind: WorkerReportKind::PollReturned,
        });

        let _ = executor.wait_for_report(worker, RunId(0));
    }

    #[test]
    #[should_panic(expected = "invalid permit")]
    fn invalid_resume_attempt_panics() {
        let executor = Executor::new(RuntimeConfig::new(16));
        let worker = executor.acquire_worker();

        executor.run_entity(RunnableEntity::Parked(ParkedStack {
            worker,
            run_id: RunId(0),
        }));
    }

    #[test]
    #[should_panic(expected = "sim::yield_sync called outside a simulated worker")]
    fn local_async_cannot_yield_synchronously() {
        let mut runtime = Runtime::new(17);
        runtime.block_on(async {
            yield_sync();
        });
    }
}
