//! Minimal asynchronous executor adapted from madsim's `sim/task` loop.

use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
    thread::{self, Thread},
    time::Duration,
};

use futures_util::FutureExt;

use crate::{seed::DstSeed, sim::Rng};

type Runnable = async_task::Runnable<NodeId>;

/// A unique identifier for a simulated node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u64);

impl NodeId {
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
/// is considered a test hang unless external system threads are explicitly
/// allowed for the current target.
pub struct Runtime {
    executor: Arc<Executor>,
}

impl Runtime {
    pub fn new(seed: DstSeed) -> anyhow::Result<Self> {
        Ok(Self {
            executor: Arc::new(Executor::new(seed)),
        })
    }

    pub fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        self.executor.block_on(future)
    }

    /// Allow parking briefly for non-DST runtime threads to wake the root task.
    ///
    /// This is currently needed by the relational target while durability still
    /// uses core's production runtime boundary.
    pub fn set_allow_system_thread(&mut self, allowed: bool) {
        self.executor.set_allow_system_thread(allowed);
    }

    pub fn handle(&self) -> Handle {
        Handle {
            executor: Arc::clone(&self.executor),
        }
    }

    pub fn create_node(&self) -> NodeId {
        self.handle().create_node()
    }

    pub fn pause(&self, node: NodeId) {
        self.handle().pause(node);
    }

    pub fn resume(&self, node: NodeId) {
        self.handle().resume(node);
    }

    pub fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle().spawn_on(node, future)
    }
}

/// Cloneable access to the simulation executor.
#[derive(Clone)]
pub struct Handle {
    executor: Arc<Executor>,
}

impl Handle {
    pub fn create_node(&self) -> NodeId {
        self.executor.create_node()
    }

    pub fn pause(&self, node: NodeId) {
        self.executor.pause(node);
    }

    pub fn resume(&self, node: NodeId) {
        self.executor.resume(node);
    }

    pub fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.executor.spawn_on(node, future)
    }
}

/// A spawned simulated task.
pub struct JoinHandle<T> {
    task: async_task::Task<T, NodeId>,
}

impl<T> JoinHandle<T> {
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

struct Executor {
    queue: Receiver,
    sender: Sender,
    nodes: Mutex<BTreeMap<NodeId, Arc<NodeState>>>,
    next_node: std::sync::atomic::AtomicU64,
    rng: Mutex<Rng>,
    allow_system_thread: AtomicBool,
}

impl Executor {
    fn new(seed: DstSeed) -> Self {
        let queue = Queue::new();
        let mut nodes = BTreeMap::new();
        nodes.insert(NodeId::MAIN, Arc::new(NodeState::default()));
        Self {
            queue: queue.receiver(),
            sender: queue.sender(),
            nodes: Mutex::new(nodes),
            next_node: std::sync::atomic::AtomicU64::new(1),
            rng: Mutex::new(Rng::new(seed)),
            allow_system_thread: AtomicBool::new(false),
        }
    }

    fn set_allow_system_thread(&self, allowed: bool) {
        self.allow_system_thread.store(allowed, Ordering::Relaxed);
    }

    fn create_node(&self) -> NodeId {
        let id = NodeId(self.next_node.fetch_add(1, Ordering::Relaxed));
        self.nodes
            .lock()
            .expect("nodes poisoned")
            .insert(id, Arc::new(NodeState::default()));
        id
    }

    fn pause(&self, node: NodeId) {
        self.node_state(node).paused.store(true, Ordering::Relaxed);
    }

    fn resume(&self, node: NodeId) {
        let state = self.node_state(node);
        state.paused.store(false, Ordering::Relaxed);

        let mut paused = state.paused_queue.lock().expect("paused queue poisoned");
        for runnable in paused.drain(..) {
            self.sender.send(runnable);
        }
    }

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

    #[track_caller]
    fn block_on<F: Future>(&self, future: F) -> F::Output {
        let _waiter = WaiterGuard::new(&self.queue, thread::current());

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

            if self.allow_system_thread.load(Ordering::Relaxed) {
                thread::park_timeout(Duration::from_millis(1));
            } else {
                panic!("no runnable tasks; all simulated tasks are blocked");
            }
        }
    }

    fn run_all_ready(&self) {
        while let Some(runnable) = self.queue.try_recv_random(&self.rng) {
            let node = *runnable.metadata();
            let state = self.node_state(node);
            if state.paused.load(Ordering::Relaxed) {
                state.paused_queue.lock().expect("paused queue poisoned").push(runnable);
                continue;
            }
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| runnable.run()));
            if let Err(payload) = result {
                std::panic::resume_unwind(payload);
            }
        }
    }

    fn node_state(&self, node: NodeId) -> Arc<NodeState> {
        self.nodes
            .lock()
            .expect("nodes poisoned")
            .get(&node)
            .cloned()
            .unwrap_or_else(|| panic!("unknown simulated node {node}"))
    }
}

#[derive(Clone, Default)]
struct NodeState {
    paused: Arc<AtomicBool>,
    paused_queue: Arc<Mutex<Vec<Runnable>>>,
}

pub async fn yield_now() {
    YieldNow { yielded: false }.await
}

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

struct WaiterGuard<'a> {
    receiver: &'a Receiver,
}

impl<'a> WaiterGuard<'a> {
    fn new(receiver: &'a Receiver, thread: Thread) -> Self {
        receiver.set_waiter(Some(thread));
        Self { receiver }
    }
}

impl Drop for WaiterGuard<'_> {
    fn drop(&mut self) {
        self.receiver.set_waiter(None);
    }
}

struct Queue {
    inner: Arc<QueueInner>,
}

#[derive(Clone)]
struct Sender {
    inner: Arc<QueueInner>,
}

#[derive(Clone)]
struct Receiver {
    inner: Arc<QueueInner>,
}

struct QueueInner {
    queue: Mutex<Vec<Runnable>>,
    waiter: Mutex<Option<Thread>>,
}

impl Queue {
    fn new() -> Self {
        Self {
            inner: Arc::new(QueueInner {
                queue: Mutex::new(Vec::new()),
                waiter: Mutex::new(None),
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
    fn send(&self, runnable: Runnable) {
        self.inner.queue.lock().expect("run queue poisoned").push(runnable);
        if let Some(thread) = self.inner.waiter.lock().expect("waiter poisoned").as_ref() {
            thread.unpark();
        }
    }
}

impl Receiver {
    fn set_waiter(&self, thread: Option<Thread>) {
        *self.inner.waiter.lock().expect("waiter poisoned") = thread;
    }

    fn try_recv_random(&self, rng: &Mutex<Rng>) -> Option<Runnable> {
        let mut queue = self.inner.queue.lock().expect("run queue poisoned");
        if queue.is_empty() {
            return None;
        }
        let idx = rng.lock().expect("rng poisoned").index(queue.len());
        Some(queue.swap_remove(idx))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use super::*;

    #[test]
    fn paused_node_does_not_run_until_resumed() {
        let mut runtime = Runtime::new(DstSeed(1)).unwrap();
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
        let mut runtime = Runtime::new(DstSeed(2)).unwrap();
        let handle = runtime.handle();

        let value = runtime.block_on(async move {
            let node = handle.create_node();
            handle.spawn_on(node, async { 11 }).await
        });

        assert_eq!(value, 11);
    }
}
