//! Minimal asynchronous executor adapted from madsim's `sim/task` loop.

use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use core::{
    fmt,
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    task::{Context, Poll},
    time::Duration,
};

use futures_util::FutureExt;
use spin::Mutex;

use crate::sim::{time::TimeHandle, Rng, RuntimeConfig};

type Runnable = async_task::Runnable<NodeId>;

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
    pub fn create_node(&self) -> NodeId {
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

    #[allow(dead_code)]
    pub(crate) fn rng(&self) -> Rng {
        self.executor.rng.clone()
    }
}

/// Cloneable access to the simulation executor.
#[derive(Clone)]
pub struct Handle {
    executor: Arc<Executor>,
}

impl Handle {
    /// Create a new simulated node owned by this runtime.
    pub fn create_node(&self) -> NodeId {
        self.executor.create_node()
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

/// A spawned simulated task.
pub struct JoinHandle<T> {
    task: async_task::Task<T, NodeId>,
}

impl<T> JoinHandle<T> {
    /// Detach the task so it continues running without awaiting its output.
    pub fn detach(self) {
        self.task.detach();
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.task).poll(cx)
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
    nodes: spin::Mutex<BTreeMap<NodeId, Arc<NodeState>>>,
    next_node: AtomicU64,
    rng: Rng,
    time: TimeHandle,
}

impl Executor {
    /// Construct a fresh executor with one default `MAIN` node.
    fn new(config: RuntimeConfig) -> Self {
        let queue = Queue::new();
        let mut nodes = BTreeMap::new();
        nodes.insert(NodeId::MAIN, Arc::new(NodeState::default()));
        Self {
            queue: queue.receiver(),
            sender: queue.sender(),
            nodes: spin::Mutex::new(nodes),
            next_node: AtomicU64::new(1),
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

    fn create_node(&self) -> NodeId {
        let id = NodeId(self.next_node.fetch_add(1, Ordering::Relaxed));
        self.nodes.lock().insert(id, Arc::new(NodeState::default()));
        id
    }

    /// Mark a node as paused so newly selected runnables are buffered.
    fn pause(&self, node: NodeId) {
        self.node_state(node).paused.store(true, Ordering::Relaxed);
    }

    /// Mark a node as runnable again and requeue any buffered tasks for it.
    fn resume(&self, node: NodeId) {
        let state = self.node_state(node);
        state.paused.store(false, Ordering::Relaxed);

        let mut paused = state.paused_queue.lock();
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
        self.node_state(node);

        let sender = self.sender.clone();
        let (runnable, task) = async_task::Builder::new()
            .metadata(node)
            .spawn(move |_| future, move |runnable| sender.send(runnable));
        runnable.schedule();

        JoinHandle { task }
    }

    /// Spawn a non-`Send` task on the single-threaded runtime.
    fn spawn_local_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.node_state(node);

        let sender = self.sender.clone();
        let (runnable, task) = unsafe {
            async_task::Builder::new()
                .metadata(node)
                .spawn_unchecked(move |_| future, move |runnable| sender.send(runnable))
        };
        runnable.schedule();

        JoinHandle { task }
    }

    #[track_caller]
    /// Run the top-level future until completion.
    ///
    /// The executor repeatedly drains runnable tasks, then advances virtual
    /// time to the next timer when the queue is empty. If neither runnable work
    /// nor timers remain, the simulation is considered deadlocked.
    fn block_on<F: Future>(&self, future: F) -> F::Output {
        let sender = self.sender.clone();
        let (runnable, task) = unsafe {
            async_task::Builder::new()
                .metadata(NodeId::MAIN)
                .spawn_unchecked(move |_| future, move |runnable| sender.send(runnable))
        };
        runnable.schedule();

        loop {
            self.run_all_ready();
            if task.is_finished() {
                return task.now_or_never().expect("finished task should resolve");
            }

            if self.time.wake_next_timer() {
                continue;
            }

            panic!("no runnable tasks; all simulated tasks are blocked");
        }
    }

    /// Drain the runnable queue, selecting tasks in deterministic RNG order.
    ///
    /// Paused-node tasks are diverted into that node's paused buffer instead of
    /// being polled immediately.
    fn run_all_ready(&self) {
        while let Some(runnable) = self.queue.try_recv_random(&self.rng) {
            let node = *runnable.metadata();
            let state = self.node_state(node);
            if state.paused.load(Ordering::Relaxed) {
                state.paused_queue.lock().push(runnable);
                continue;
            }
            runnable.run();
        }
    }

    /// Look up the scheduling state for a node, panicking if the node is unknown.
    fn node_state(&self, node: NodeId) -> Arc<NodeState> {
        self.nodes
            .lock()
            .get(&node)
            .cloned()
            .unwrap_or_else(|| panic!("unknown simulated node {node}"))
    }
}

/// Per-node scheduler state shared by tasks assigned to that node.
#[derive(Clone, Default)]
struct NodeState {
    paused: Arc<AtomicBool>,
    paused_queue: Arc<Mutex<Vec<Runnable>>>,
}

/// Yield back to the scheduler once.
///
/// This is the smallest explicit interleaving point available to simulated
/// tasks when they need to give other runnables a chance to execute.
pub async fn yield_now() {
    YieldNow { yielded: false }.await
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

/// Shared runnable queue used by the simulation executor.
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
    queue: Mutex<Vec<Runnable>>,
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
    fn send(&self, runnable: Runnable) {
        self.inner.queue.lock().push(runnable);
    }
}

impl Receiver {
    /// Remove one runnable using the runtime RNG to choose among ready tasks.
    fn try_recv_random(&self, rng: &Rng) -> Option<Runnable> {
        let mut queue = self.inner.queue.lock();
        if queue.is_empty() {
            return None;
        }
        let idx = rng.index(queue.len());
        Some(queue.swap_remove(idx))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };

    use super::*;
    use crate::sim::RuntimeConfig;

    #[test]
    fn paused_node_does_not_run_until_resumed() {
        let mut runtime = Runtime::new(1);
        let node = runtime.create_node();
        runtime.pause(node);

        let runs = Arc::new(AtomicUsize::new(0));
        let task_runs = Arc::clone(&runs);
        let task = runtime.spawn_on(node, async move {
            task_runs.fetch_add(1, Ordering::SeqCst);
            7
        });

        runtime.block_on(async {
            yield_now().await;
        });
        assert_eq!(runs.load(Ordering::SeqCst), 0);

        runtime.resume(node);
        assert_eq!(runtime.block_on(task), 7);
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn handle_can_spawn_onto_node_from_simulated_task() {
        let mut runtime = Runtime::new(2);
        let handle = runtime.handle();

        let value = runtime.block_on(async move {
            let node = handle.create_node();
            handle.spawn_on(node, async { 11 }).await
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
            let mut rng = Rng::new(77);
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

    #[cfg(feature = "simulation-std")]
    #[test]
    fn current_handle_can_spawn_local_task_inside_runtime() {
        assert!(crate::adapter::sim_std::current_handle().is_none());

        let mut runtime = Runtime::new(5);
        let value = crate::adapter::sim_std::block_on(&mut runtime, async {
            let handle =
                crate::adapter::sim_std::current_handle().expect("sim handle should be present inside block_on");
            let node = handle.create_node();
            let captured = std::rc::Rc::new(17);
            handle
                .spawn_local_on(node, async move {
                    yield_now().await;
                    *captured
                })
                .await
        });

        assert_eq!(value, 17);
        assert!(crate::adapter::sim_std::current_handle().is_none());
    }

    #[cfg(feature = "simulation-std")]
    #[test]
    fn check_determinism_runs_future_twice() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        CALLS.store(0, Ordering::SeqCst);

        let value = crate::adapter::sim_std::check_determinism(3, || async {
            CALLS.fetch_add(1, Ordering::SeqCst);
            yield_now().await;
            13
        });

        assert_eq!(value, 13);
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
    }

    #[cfg(feature = "simulation-std")]
    #[test]
    #[should_panic(expected = "non-determinism detected")]
    fn check_determinism_rejects_different_scheduler_sequence() {
        static FIRST_RUN: AtomicBool = AtomicBool::new(true);
        FIRST_RUN.store(true, Ordering::SeqCst);

        crate::adapter::sim_std::check_determinism(4, || async {
            if FIRST_RUN.swap(false, Ordering::SeqCst) {
                yield_now().await;
            }
        });
    }
}
