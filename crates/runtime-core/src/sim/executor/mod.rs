use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use core::{
    fmt,
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    task::{Context, Poll, Waker},
    time::Duration,
};

use spin::Mutex;

use crate::sim::executor::task::Abortable;

use super::rng::Ratio;
use super::{net, time::TimeHandle, Rng};

mod task;
pub use task::{AbortHandle, JoinError, JoinHandle};

type Runnable = async_task::Runnable<TaskMeta>;

/// Immutable scheduling metadata attached to every simulated task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TaskMeta {
    node: NodeId,
    generation: u64,
}

impl TaskMeta {
    fn new(node: NodeId, generation: u64) -> Self {
        Self { node, generation }
    }
}

const READY_TASK_BUDGET: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub seed: u64,
    pub network: Option<net::Options>,
    pub node_faults: NodeFaultOptions,
}

impl RuntimeConfig {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            network: None,
            node_faults: NodeFaultOptions::default(),
        }
    }

    pub fn with_network(mut self, network: net::Options) -> Self {
        self.network = network.into();
        self
    }

    pub fn with_node_faults(mut self, node_faults: NodeFaultOptions) -> Self {
        self.node_faults = node_faults;
        self
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeFaultOptions {
    /// Probability per fault tick that a running node crashes.
    pub crash_node_probability: Ratio,
    /// Probability per fault tick that a crashed node restarts.
    pub restart_node_probability: Ratio,
    /// Probability per fault tick that a running node pauses.
    pub pause_node_probability: Ratio,
    /// Probability per fault tick that a paused node resumes.
    pub unpause_node_probability: Ratio,
}

impl Default for NodeFaultOptions {
    fn default() -> Self {
        Self {
            crash_node_probability: Ratio::ZERO,
            restart_node_probability: Ratio::ZERO,
            pause_node_probability: Ratio::ZERO,
            unpause_node_probability: Ratio::ZERO,
        }
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

    /// Return the simulated network endpoint for this node.
    pub fn net(&self) -> Option<net::NodeNetwork> {
        self.handle.network().map(|net| net.on_node(self.id))
    }

    /// Crash this node and invalidate all tasks spawned before the crash.
    pub fn crash(&self) {
        self.handle.crash_node(self.id);
    }

    /// Pause scheduling for this node.
    pub fn pause(&self) {
        self.handle.pause(self.id);
    }

    /// Resume scheduling for this node.
    pub fn resume(&self) {
        self.handle.resume(self.id);
    }

    /// Restart this node and invalidate all tasks spawned before the restart.
    pub fn restart(&self) {
        self.handle.restart_node(self.id);
    }

    /// Spawn a future onto this simulated node.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.handle.executor.assert_main_or_node(self.id);
        self.handle.executor.spawn_on(self.id, future)
    }

    /// Spawn a future onto this simulated node.
    pub fn spawn_local<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.spawn(future)
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

    /// Spawn a future onto the currently running node, or `MAIN` outside node work.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.executor.spawn(future)
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
    #[doc(hidden)]
    pub fn enable_determinism_log(&self) {
        self.executor.rng.enable_determinism_log();
    }

    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn enable_determinism_check(&self, log: super::DeterminismLog) {
        self.executor.rng.enable_determinism_check(log);
    }

    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn take_determinism_log(&self) -> Option<super::DeterminismLog> {
        self.executor.rng.take_determinism_log()
    }

    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn finish_determinism_check(&self) -> Result<(), alloc::string::String> {
        self.executor.rng.finish_determinism_check()
    }
}

/// Cloneable access to the simulation executor.
#[derive(Clone)]
pub struct Handle {
    executor: Arc<Executor>,
}

impl Handle {
    /// Return the shared simulated network for this runtime.
    pub fn network(&self) -> Option<net::Network> {
        self.executor.net.clone()
    }

    /// Create a new simulated node owned by this runtime.
    pub fn create_node(&self) -> NodeBuilder {
        NodeBuilder {
            handle: self.clone(),
            config: NodeConfig::default(),
        }
    }

    fn node_config(&self, node: NodeId) -> Arc<NodeConfig> {
        self.executor.node_config(node)
    }

    fn build_node(&self, config: NodeConfig) -> Node {
        let id = self.executor.create_node(config);
        let config = self.node_config(id);
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

    /// Crash a node until it is restarted.
    pub fn crash_node(&self, node: NodeId) {
        self.executor.crash_node(node);
    }

    /// Resume scheduling for a node and requeue any buffered tasks for it.
    pub fn resume(&self, node: NodeId) {
        self.executor.resume(node);
    }

    /// Restart a node and invalidate all tasks spawned before the restart.
    pub fn restart_node(&self, node: NodeId) {
        self.executor.restart_node(node);
    }

    /// Spawn a future onto the currently running node, or `MAIN` outside node work.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.executor.spawn(future)
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
    pub fn sleep(&self, duration: Duration) -> super::time::Sleep {
        self.executor.time.sleep(duration)
    }

    /// Race a future against a virtual-time timeout.
    pub async fn timeout<T>(
        &self,
        duration: Duration,
        future: impl Future<Output = T>,
    ) -> Result<T, super::time::TimeoutElapsed> {
        self.executor.time.timeout(duration, future).await
    }

    /// Yield this task back to the simulation scheduler once.
    pub async fn yield_now(&self) {
        yield_now().await
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.executor.block_on(future)
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
    current_task: Mutex<Option<TaskMeta>>,
    nodes: spin::Mutex<BTreeMap<NodeId, Arc<NodeRecord>>>,
    node_faults: NodeFaultOptions,
    next_node: AtomicU64,
    rng: Rng,
    time: TimeHandle,
    net: Option<net::Network>,
}

impl Executor {
    /// Construct a fresh executor with one default `MAIN` node.
    fn new(config: RuntimeConfig) -> Self {
        let queue = Queue::new();
        let mut nodes = BTreeMap::new();
        let time = TimeHandle::new();
        let rng = Rng::new(config.seed);

        nodes.insert(NodeId::MAIN, Arc::new(NodeRecord::new(NodeConfig::default())));

        let net = config
            .network
            .map(|config| net::Network::new(time.clone(), rng.clone(), config));
        if let Some(net) = &net {
            net.register_node(NodeId::MAIN);
        }

        Self {
            queue: queue.receiver(),
            sender: queue.sender(),
            current_task: Mutex::new(None),
            nodes: spin::Mutex::new(nodes),
            node_faults: config.node_faults,
            next_node: AtomicU64::new(1),
            rng,
            time,
            net,
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
        self.nodes.lock().insert(id, Arc::new(NodeRecord::new(config)));

        if let Some(net) = &self.net {
            net.register_node(id);
        }

        id
    }

    /// Mark a node as paused so newly selected runnables are buffered.
    fn pause(&self, node: NodeId) {
        assert_ne!(node, NodeId::MAIN, "cannot pause the main simulation node");

        self.node_state(node).paused.store(true, Ordering::Relaxed);
        if let Some(net) = &self.net {
            net.isolate_node(node);
        }
    }

    /// Mark a node as crashed until it is restarted.
    fn crash_node(&self, node: NodeId) {
        assert_ne!(node, NodeId::MAIN, "cannot crash the main simulation node");

        let state = self.node_state(node);
        state.crashed.store(true, Ordering::Release);
        state.paused.store(false, Ordering::Release);
        state.bump_generation();
        let stale_runnables = core::mem::take(&mut *state.paused_queue.lock());
        drop(stale_runnables);
        if let Some(net) = &self.net {
            net.isolate_node(node);
        }
    }

    /// Mark a node as runnable again and requeue any buffered tasks for it.
    fn resume(&self, node: NodeId) {
        let state = self.node_state(node);
        state.paused.store(false, Ordering::Relaxed);
        if state.is_crashed() {
            let stale_runnables = core::mem::take(&mut *state.paused_queue.lock());
            drop(stale_runnables);
            return;
        }
        let runnables = core::mem::take(&mut *state.paused_queue.lock());
        for runnable in runnables {
            self.sender.send(runnable);
        }
        if let Some(net) = &self.net {
            net.unisolate_node(node);
        }
    }

    /// Mark a crashed node as running again.
    fn restart_node(&self, node: NodeId) {
        assert_ne!(node, NodeId::MAIN, "cannot restart the main simulation node");

        let state = self.node_state(node);
        state.crashed.store(false, Ordering::Release);
        state.paused.store(false, Ordering::Release);
        state.bump_generation();
        let stale_runnables = core::mem::take(&mut *state.paused_queue.lock());
        drop(stale_runnables);
        if let Some(net) = &self.net {
            net.unisolate_node(node);
        }
    }

    /// Spawn a task onto the node whose task is currently being polled.
    fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.spawn_on(self.current_node(), future)
    }

    /// Spawn a task and enqueue its runnable on the shared runtime queue.
    fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let abort = AbortHandle::new();
        let abortable = Abortable::new(future, abort.clone());
        let sender = self.sender.clone();
        let (runnable, task) = unsafe {
            async_task::Builder::new()
                .metadata(self.task_meta(node))
                .spawn_unchecked(move |_| abortable, move |runnable| sender.send(runnable))
        };
        runnable.schedule();

        JoinHandle {
            task: task.fallible(),
            abort,
        }
    }

    #[track_caller]
    /// Run the top-level future until completion.
    ///
    /// The executor polls a bounded random batch of runnable tasks, samples
    /// simulated fault sources at one captured instant, then advances virtual
    /// time only when no current-time source can make progress. If neither
    /// runnable work nor timers remain, the simulation is considered deadlocked.
    fn block_on<F: Future>(&self, future: F) -> F::Output {
        let sender = self.sender.clone();
        let (runnable, mut task) = unsafe {
            async_task::Builder::new()
                .metadata(self.task_meta(NodeId::MAIN))
                .spawn_unchecked(move |_| future, move |runnable| sender.send(runnable))
        };
        runnable.schedule();

        loop {
            if let Some(output) = poll_finished_task(&mut task) {
                return output;
            }

            let task_progress = self.run_ready_budget(READY_TASK_BUDGET);
            if let Some(output) = poll_finished_task(&mut task) {
                return output;
            }

            let now = self.time.now();
            let node_progress = self.node_fault_ticks(now);
            let network_progress = self.net_tick(now);
            let timer_progress = self.time.wake_due_timers();
            if task_progress || node_progress || network_progress || timer_progress {
                continue;
            }

            if self.advance_to_next_network_deadline(now) || self.time.wake_next_timer() {
                continue;
            }

            panic!("no runnable tasks; all simulated tasks are blocked");
        }
    }

    fn net_tick(&self, now: Duration) -> bool {
        if let Some(net) = &self.net {
            net.tick(now)
        } else {
            false
        }
    }

    fn next_network_delivery_deadline(&self, now: Duration) -> Option<Duration> {
        self.net.as_ref().and_then(|net| net.next_delivery_deadline(now))
    }

    fn advance_to_next_network_deadline(&self, now: Duration) -> bool {
        let Some(network_deadline) = self.next_network_delivery_deadline(now) else {
            return false;
        };

        if let Some(timer_deadline) = self.time.next_timer_deadline()
            && network_deadline > timer_deadline
        {
            return false;
        }

        self.time.advance_to(network_deadline)
    }

    fn node_fault_ticks(&self, now: Duration) -> bool {
        let options = self.node_faults;

        let nodes = {
            let nodes = self.nodes.lock();
            nodes
                .keys()
                .copied()
                .filter(|node| *node != NodeId::MAIN)
                .collect::<Vec<_>>()
        };

        for node in nodes {
            if self.node_crash_tick(node, options, now) || self.node_pause_tick(node, options, now) {
                return true;
            }
        }

        false
    }

    fn node_crash_tick(&self, node: NodeId, options: NodeFaultOptions, _now: Duration) -> bool {
        let record = self.node_state(node);
        if record.crashed.load(Ordering::Acquire) {
            if self.rng.buggify_ratio(options.restart_node_probability) {
                self.restart_node(node);
                return true;
            }
        } else if !record.paused.load(Ordering::Relaxed) && self.rng.buggify_ratio(options.crash_node_probability) {
            self.crash_node(node);
            return true;
        }

        false
    }

    fn node_pause_tick(&self, node: NodeId, options: NodeFaultOptions, _now: Duration) -> bool {
        let record = self.node_state(node);
        if record.crashed.load(Ordering::Acquire) {
            return false;
        }

        if record.paused.load(Ordering::Relaxed) {
            if self.rng.buggify_ratio(options.unpause_node_probability) {
                self.resume(node);
                true
            } else {
                false
            }
        } else if self.rng.buggify_ratio(options.pause_node_probability) {
            self.pause(node);
            true
        } else {
            false
        }
    }

    /// Poll a bounded batch from the runnable queue in deterministic RNG order.
    ///
    /// Returning to the outer scheduler after a fixed quantum lets timers and
    /// network deliveries make progress even when CPU-ready tasks keep
    /// re-scheduling themselves. Paused-node tasks are diverted into that node's
    /// paused buffer instead of being polled immediately.
    fn run_ready_budget(&self, budget: usize) -> bool {
        assert!(budget > 0, "ready task budget must be non-zero");

        let mut progressed = false;
        for _ in 0..budget {
            let Some(runnable) = self.queue.try_recv_random(&self.rng) else {
                break;
            };

            progressed = true;
            let meta = *runnable.metadata();
            let state = self.node_state(meta.node);
            if !state.is_current_generation(meta.generation) || state.is_crashed() {
                drop(runnable);
                continue;
            }
            if state.is_paused() {
                state.paused_queue.lock().push(runnable);
                continue;
            }
            let _current_task = self.enter_current_task(meta);
            runnable.run();
            // Advance virtual time by 100ns-1us per task poll to model execution cost.
            // Using the runtime RNG keeps overhead deterministic by seed.
            let nanos = 100 + (self.rng.next_u64() % 901);
            self.time.advance(Duration::from_nanos(nanos));
        }
        progressed
    }

    /// Look up the record for a node, panicking if the node is unknown.
    fn node_record(&self, node: NodeId) -> Arc<NodeRecord> {
        self.nodes
            .lock()
            .get(&node)
            .cloned()
            .unwrap_or_else(|| panic!("unknown simulated node {node}"))
    }

    fn node_config(&self, node: NodeId) -> Arc<NodeConfig> {
        self.node_record(node).config.clone()
    }

    fn task_meta(&self, node: NodeId) -> TaskMeta {
        let state = self.node_state(node);
        TaskMeta::new(node, state.generation())
    }

    fn current_node(&self) -> NodeId {
        self.current_task
            .lock()
            .as_ref()
            .map(|meta| meta.node)
            .unwrap_or(NodeId::MAIN)
    }

    fn assert_main_or_node(&self, node: NodeId) {
        let caller = self.current_node();
        assert!(
            caller == NodeId::MAIN || caller == node,
            "node {caller} cannot spawn task on node {node}"
        );
    }

    fn enter_current_task(&self, meta: TaskMeta) -> CurrentTaskGuard<'_> {
        let mut current = self.current_task.lock();
        // The executor must not poll another runnable while one task's node
        // context is installed; otherwise ambient spawn would inherit the
        // wrong node/generation after reentrant scheduling.
        assert!(current.is_none(), "nested simulated task polling");
        *current = Some(meta);
        CurrentTaskGuard { executor: self }
    }

    fn node_state(&self, node: NodeId) -> Arc<NodeState> {
        self.node_record(node).state.clone()
    }
}

struct CurrentTaskGuard<'a> {
    executor: &'a Executor,
}

impl Drop for CurrentTaskGuard<'_> {
    fn drop(&mut self) {
        let current = self.executor.current_task.lock().take();
        assert!(current.is_some(), "current simulated task guard dropped without task");
    }
}

fn poll_finished_task<T>(task: &mut async_task::Task<T, TaskMeta>) -> Option<T> {
    if !task.is_finished() {
        return None;
    }

    let waker = Waker::noop();
    match Pin::new(task).poll(&mut Context::from_waker(waker)) {
        Poll::Ready(output) => Some(output),
        Poll::Pending => unreachable!("task.is_finished() was true"),
    }
}

/// Complete executor record for a simulated node.
struct NodeRecord {
    config: Arc<NodeConfig>,
    state: Arc<NodeState>,
}

impl NodeRecord {
    fn new(config: NodeConfig) -> Self {
        Self {
            config: Arc::new(config),
            state: Arc::new(NodeState::default()),
        }
    }
}

/// Per-node scheduler state shared by tasks assigned to that node.
#[derive(Clone)]
struct NodeState {
    paused: Arc<AtomicBool>,
    crashed: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    paused_queue: Arc<Mutex<Vec<Runnable>>>,
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            paused: Arc::new(AtomicBool::new(false)),
            crashed: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
            paused_queue: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl NodeState {
    fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn is_current_generation(&self, generation: u64) -> bool {
        self.generation() == generation
    }

    fn is_crashed(&self) -> bool {
        self.crashed.load(Ordering::Relaxed)
    }

    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
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

    struct DropFlag(Arc<AtomicUsize>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct PendingUntilDropped {
        _drop: DropFlag,
    }

    impl Future for PendingUntilDropped {
        type Output = u32;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }

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
    #[should_panic(expected = "cannot spawn task on node")]
    fn node_cannot_spawn_task_on_another_node() {
        let mut runtime = Runtime::new(3);
        let node_a = runtime.create_node().name("a").build();
        let node_b = runtime.create_node().name("b").build();

        let task = node_a.spawn(async move {
            let _child = node_b.spawn(async {});
        });

        runtime.block_on(task).expect("parent task should panic first");
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

    #[test]
    fn crashed_node_drops_sleeping_task_and_join_errors() {
        let mut runtime = Runtime::new(11);
        let handle = runtime.handle();
        let node = runtime.create_node().name("crash").build();
        let started = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicUsize::new(0));

        let task = node.spawn({
            let handle = handle.clone();
            let started = Arc::clone(&started);
            let dropped = Arc::clone(&dropped);
            async move {
                let _drop = DropFlag(dropped);
                started.store(true, Ordering::SeqCst);
                handle.sleep(Duration::from_secs(1)).await;
                99
            }
        });

        runtime.block_on({
            let started = Arc::clone(&started);
            async move {
                while !started.load(Ordering::SeqCst) {
                    yield_now().await;
                }
            }
        });
        assert_eq!(dropped.load(Ordering::SeqCst), 0);

        node.crash();
        let err = runtime.block_on(task).expect_err("crashed task should be cancelled");

        assert_eq!(err, JoinError);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn task_spawned_on_crashed_node_is_dropped_and_join_errors() {
        let mut runtime = Runtime::new(12);
        let node = runtime.create_node().name("crashed").build();
        let dropped = Arc::new(AtomicUsize::new(0));

        node.crash();
        let task = node.spawn(PendingUntilDropped {
            _drop: DropFlag(Arc::clone(&dropped)),
        });

        let err = runtime.block_on(task).expect_err("crashed task should be cancelled");

        assert_eq!(err, JoinError);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn restart_invalidates_tasks_from_previous_generations() {
        let mut runtime = Runtime::new(13);
        let node = runtime.create_node().name("restart").build();
        let runs = Arc::new(AtomicUsize::new(0));

        let before_crash = node.spawn({
            let runs = Arc::clone(&runs);
            async move {
                runs.fetch_add(1, Ordering::SeqCst);
                1
            }
        });
        node.crash();
        node.restart();
        let err = runtime
            .block_on(before_crash)
            .expect_err("pre-crash task should be cancelled after restart");
        assert_eq!(err, JoinError);
        assert_eq!(runs.load(Ordering::SeqCst), 0);

        node.crash();
        let during_crash = node.spawn({
            let runs = Arc::clone(&runs);
            async move {
                runs.fetch_add(1, Ordering::SeqCst);
                2
            }
        });
        node.restart();
        let err = runtime
            .block_on(during_crash)
            .expect_err("task spawned while crashed should be cancelled after restart");
        assert_eq!(err, JoinError);
        assert_eq!(runs.load(Ordering::SeqCst), 0);

        let after_restart = node.spawn({
            let runs = Arc::clone(&runs);
            async move {
                runs.fetch_add(1, Ordering::SeqCst);
                3
            }
        });
        assert_eq!(
            runtime.block_on(after_restart).expect("post-restart task should run"),
            3
        );
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn block_on_can_spawn_local_task_with_explicit_handle() {
        let mut runtime = Runtime::new(5);
        let handle = runtime.handle();
        let node = handle.create_node().name("local").build();
        let value = runtime.block_on(async move {
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
    fn block_on_returns_while_background_task_stays_ready() {
        let mut runtime = Runtime::new(10);
        let handle = runtime.handle();
        let node = runtime.create_node().name("hot").build();
        let _hot = node.spawn(async {
            loop {
                yield_now().await;
            }
        });

        let value = runtime.block_on(async move {
            handle.sleep(Duration::from_micros(1)).await;
            42
        });

        assert_eq!(value, 42);
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
}
