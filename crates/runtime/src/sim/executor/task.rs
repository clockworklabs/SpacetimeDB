use alloc::sync::Arc;
use core::{
    fmt,
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll, Waker},
};

use spin::Mutex;

use super::NodeId;

/// A spawned simulated task.
///
/// Two handles reference the same underlying allocation:
/// - `JoinHandle` awaits the output and holds an `AbortHandle` for cancellation.
/// - The executor holds the `Runnable` (not visible here).
pub struct JoinHandle<T> {
    // async_task::Task owns a shared heap-allocated cell that holds the future,
    // its output, metadata (NodeId), and waker. Polling it drives the future
    // to completion. Dropping it without detach cancels the future.
    pub(crate) task: async_task::Task<Result<T, JoinError>, NodeId>,
    // Clone of the same AbortHandle that Abortable holds inside the task.
    pub(crate) abort: AbortHandle,
}

impl<T> JoinHandle<T> {
    /// Return a handle that can cancel this task.
    pub fn abort_handle(&self) -> AbortHandle {
        self.abort.clone()
    }

    /// Drop the join handle without cancelling the task.
    pub fn detach(self) {
        // async_task::Task::detach makes Drop a no-op — the future keeps running.
        self.task.detach();
    }

    /// Poll the underlying async_task::Task for its output.
    pub(crate) fn poll_join(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<T, JoinError>> {
        // async_task::Task implements Future. Polling it drives the wrapped
        // Abortable future inside the executor.
        Pin::new(&mut self.task).poll(cx)
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.as_mut().poll_join(cx)
    }
}

/// Two-phase cancellation for a simulated task.
///
/// [`AbortHandle`] and [`Abortable`] work together:
/// - `abort()` sets an atomic flag and wakes the task so it gets polled.
/// - On the next poll, `Abortable` checks the flag and returns `Err(JoinError)`.
/// - `JoinHandle::poll` reads that error and surfaces it to the awaiting code.
/// - The task's future is dropped naturally when `Abortable` returns `Err`.
///
/// `abort()` is thread-safe — it can be called from any task or node, and the
/// waker ensures the target task is re-scheduled even if it was blocked on I/O
/// or a timer.
#[derive(Clone)]
pub struct AbortHandle {
    state: Arc<AbortState>,
}

impl AbortHandle {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(AbortState::new()),
        }
    }

    pub fn abort(&self) {
        // Step 1: atomically mark the task as aborted.
        self.state.aborted.store(true, Ordering::Relaxed);
        // Step 2: wake the task so the executor re-schedules it for polling.
        // If the task is blocked on a timer, the waker cancels that wait.
        if let Some(waker) = self.state.waker.lock().take() {
            waker.wake();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JoinError;

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("task was cancelled")
    }
}

#[cfg(feature = "simulation")]
impl std::error::Error for JoinError {}

// Shared state between AbortHandle and Abortable.
struct AbortState {
    // Set to true by AbortHandle::abort(), read by Abortable::poll().
    aborted: AtomicBool,
    // The executor's waker, registered by Abortable on every poll.
    // Stored so abort() can wake the task even if it's waiting on I/O.
    waker: Mutex<Option<Waker>>,
}

impl AbortState {
    fn new() -> Self {
        Self {
            aborted: AtomicBool::new(false),
            waker: Mutex::new(None),
        }
    }
}

/// Wraps a future so it can be cancelled via an [`AbortHandle`].
///
/// The executor wraps every spawned future in `Abortable`. On each poll it
/// checks the cancellation flag before progressing the inner future.
pub(crate) struct Abortable<F> {
    future: F,
    abort: AbortHandle,
}

impl<F> Abortable<F> {
    pub(crate) fn new(future: F, abort: AbortHandle) -> Self {
        Self { future, abort }
    }
}

impl<F: Future> Future for Abortable<F> {
    type Output = Result<F::Output, JoinError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Check cancellation before doing any work.
        if self.abort.state.aborted.load(Ordering::Relaxed) {
            return Poll::Ready(Err(JoinError));
        }

        // Register the waker so abort() can wake this task.
        self.abort.state.waker.lock().replace(cx.waker().clone());

        // SAFETY: The `Abortable` struct is `#[repr(transparent)]`-like in its
        // pin projection: `future` is behind the cancellation fields (`abort`)
        // that are never moved once pinned. We use `map_unchecked_mut` to project
        // through the struct layout, which is safe because:
        //   1. `future` is a direct field of `Abortable` — no indirection.
        //   2. `abort` is never moved or modified in ways that would change the
        //      address of `future` relative to `self`.
        //   3. The caller guarantees `self` stays pinned for the lifetime of the
        //      future.
        let mut future = unsafe { self.map_unchecked_mut(|this| &mut this.future) };
        future.as_mut().poll(cx).map(Ok)
    }
}
