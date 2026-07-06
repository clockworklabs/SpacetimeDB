#[cfg(feature = "simulation")]
extern crate alloc;

use core::{
    fmt,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

#[cfg(feature = "simulation")]
pub mod sim;
#[cfg(feature = "simulation")]
pub mod sim_std;

pub type TokioHandle = tokio::runtime::Handle;
pub type TokioRuntime = tokio::runtime::Runtime;
pub type TokioRuntimeBuilder = tokio::runtime::Builder;

// We intentionally expose a small subset of `tokio::sync` for use under the
// simulation backend. Tokio's async synchronization primitives are runtime-agnostic: they
// can be polled by this executor instead of a Tokio runtime.
//
// Runtime-agnostic does not translate to deterministic by itself. For
// deterministic simulation, `Waker`s must be invoked by a task running on the
// deterministic executor. For the exports below, that means sends, receives,
// closes, drops of senders/receivers, and watch updates must be driven by
// simulated tasks.
//
// Anything outside the simulated runtime that invokes a stored `Waker`
// bypasses the deterministic executor. This includes Tokio timers,
// OS/kernel readiness routed through another runtime, and blocking threads.
//
// Tokio documents `*_timeout` methods as non-runtime-agnostic because they
// require Tokio's timer; in this subset, that includes
// `mpsc::Sender::send_timeout`.
//
// Also avoid blocking methods. The blocking methods currently reachable from
// this subset are `mpsc::Sender::blocking_send`,
// `mpsc::Receiver::blocking_recv`, `mpsc::Receiver::blocking_recv_many`,
// `mpsc::UnboundedReceiver::blocking_recv`, and
// `mpsc::UnboundedReceiver::blocking_recv_many`. These block or park the
// calling OS thread, which is outside the simulation runtime.
pub mod sync {
    // TODO: Remove unbounded channels as resources should be bounded.
    pub use tokio::sync::mpsc;
    pub use tokio::sync::watch;
}

pub enum Handle {
    Tokio(TokioHandle),
    #[cfg(feature = "simulation")]
    Simulation(sim::Handle),
}

impl Clone for Handle {
    fn clone(&self) -> Self {
        match self {
            Self::Tokio(handle) => Self::Tokio(handle.clone()),
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => Self::Simulation(handle.clone()),
        }
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tokio(_) => f.write_str("Handle::Tokio"),
            #[cfg(feature = "simulation")]
            Self::Simulation(_) => f.write_str("Handle::Simulation"),
        }
    }
}

pub struct JoinHandle<T> {
    inner: JoinHandleInner<T>,
}

pub struct AbortHandle {
    inner: AbortHandleInner,
}

enum JoinHandleInner<T> {
    Tokio(tokio::task::JoinHandle<T>),
    #[cfg(feature = "simulation")]
    Simulation(sim::JoinHandle<T>),
    // Placeholder variant left behind whenever the real backend handle needs
    // to be extracted from this enum while keeping the `JoinHandle` alive.
    //
    // This happens in two cases:
    //
    // 1. After the task output has been yielded — the backend handle no longer
    //    owns `T`, so we swap it out for a neutral placeholder rather than
    //    leave a semantically-invalid variant in place.
    // 2. In `Drop`, so we can call `detach()` on the simulation handle (which
    //    keeps the task alive) while tokio handles can just be dropped.
    //
    // `PhantomData<T>` is here only to keep the enum covariant in `T`.
    Detached(PhantomData<T>),
}

enum AbortHandleInner {
    Tokio(tokio::task::AbortHandle),
    #[cfg(feature = "simulation")]
    Simulation(sim::AbortHandle),
}

#[derive(Debug)]
pub struct JoinError {
    inner: JoinErrorInner,
}

#[derive(Debug)]
enum JoinErrorInner {
    Tokio(tokio::task::JoinError),
    #[cfg(feature = "simulation")]
    Simulation(sim::JoinError),
}

impl From<tokio::task::AbortHandle> for AbortHandle {
    fn from(handle: tokio::task::AbortHandle) -> Self {
        Self {
            inner: AbortHandleInner::Tokio(handle),
        }
    }
}

impl AbortHandle {
    pub fn abort(&self) {
        match &self.inner {
            AbortHandleInner::Tokio(handle) => handle.abort(),
            #[cfg(feature = "simulation")]
            AbortHandleInner::Simulation(handle) => handle.abort(),
        }
    }
}

impl JoinErrorInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tokio(err) => fmt::Display::fmt(err, f),
            #[cfg(feature = "simulation")]
            Self::Simulation(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl JoinError {
    pub fn is_panic(&self) -> bool {
        match &self.inner {
            JoinErrorInner::Tokio(err) => err.is_panic(),
            #[cfg(feature = "simulation")]
            JoinErrorInner::Simulation(err) => err.is_panic(),
        }
    }
}

impl std::error::Error for JoinError {}

impl<T> JoinHandleInner<T> {
    fn abort_handle(&self) -> AbortHandle {
        match self {
            Self::Tokio(handle) => AbortHandle {
                inner: AbortHandleInner::Tokio(handle.abort_handle()),
            },
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => AbortHandle {
                inner: AbortHandleInner::Simulation(handle.abort_handle()),
            },
            Self::Detached(_) => unreachable!("abort_handle called on a completed handle"),
        }
    }

    fn poll_result(&mut self, cx: &mut Context<'_>) -> Poll<Result<T, JoinError>> {
        match self {
            Self::Tokio(handle) => match Pin::new(handle).poll(cx) {
                Poll::Ready(Ok(output)) => Poll::Ready(Ok(output)),
                Poll::Ready(Err(err)) => Poll::Ready(Err(JoinError {
                    inner: JoinErrorInner::Tokio(err),
                })),
                Poll::Pending => Poll::Pending,
            },
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => match Pin::new(handle).poll_join(cx) {
                Poll::Ready(Ok(output)) => Poll::Ready(Ok(output)),
                Poll::Ready(Err(err)) => Poll::Ready(Err(JoinError {
                    inner: JoinErrorInner::Simulation(err),
                })),
                Poll::Pending => Poll::Pending,
            },
            Self::Detached(_) => unreachable!("poll_result called on a completed handle"),
        }
    }
}

impl<T> JoinHandle<T> {
    pub fn abort_handle(&self) -> AbortHandle {
        self.inner.abort_handle()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.inner.poll_result(cx) {
            Poll::Ready(Ok(output)) => {
                self.inner = JoinHandleInner::Detached(PhantomData);
                Poll::Ready(Ok(output))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        let inner = core::mem::replace(&mut self.inner, JoinHandleInner::Detached(PhantomData));
        match inner {
            #[cfg(feature = "simulation")]
            JoinHandleInner::Simulation(handle) => handle.detach(),
            // For Tokio (and Detached), dropping the handle does not cancel the task.
            other => drop(other),
        }
    }
}

impl<T> Unpin for JoinHandle<T> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeTimeout;

impl fmt::Display for RuntimeTimeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("runtime operation timed out")
    }
}

impl std::error::Error for RuntimeTimeout {}

impl Handle {
    pub fn tokio(handle: TokioHandle) -> Self {
        Self::Tokio(handle)
    }

    pub fn tokio_current() -> Self {
        Self::tokio(TokioHandle::current())
    }
}

#[cfg(feature = "simulation")]
impl Handle {
    pub fn simulation(handle: sim::Handle) -> Self {
        Self::Simulation(handle)
    }

    pub fn on_simulation_node(&self, node: sim::NodeId) -> Self {
        match self {
            Self::Tokio(_) => panic!("Handle::on_simulation_node requires a simulation runtime"),
            Self::Simulation(handle) => Self::Simulation(handle.on_node(node)),
        }
    }

    pub fn create_simulation_node(&self) -> Option<sim::NodeBuilder> {
        match self {
            Self::Tokio(_) => panic!("Handle::create_simulation_node requires a simulation runtime"),
            Self::Simulation(handle) => Some(handle.create_node()),
        }
    }

    pub fn drain_simulation_task_panics(&self) -> Vec<sim::TaskPanic> {
        match self {
            Self::Tokio(_) => panic!("Handle::drain_simulation_task_panics requires a simulation runtime"),
            Self::Simulation(handle) => handle.drain_task_panics(),
        }
    }
}

impl Handle {
    pub fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static) -> JoinHandle<T> {
        match self {
            Self::Tokio(handle) => JoinHandle {
                inner: JoinHandleInner::Tokio(handle.spawn(future)),
            },
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => JoinHandle {
                inner: JoinHandleInner::Simulation(handle.spawn(future)),
            },
        }
    }

    pub async fn spawn_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        match self {
            Self::Tokio(_) => tokio::task::spawn_blocking(f)
                .await
                .unwrap_or_else(|e| match e.try_into_panic() {
                    Ok(panic_payload) => std::panic::resume_unwind(panic_payload),
                    Err(e) => panic!("Unexpected JoinError: {e}"),
                }),
            // This is only a facade placeholder for simulation today. It
            // delegates to a normal simulated task, so the closure still runs
            // on the single executor thread and can block overall runtime
            // progress. Callers should not expect blocking-pool semantics on
            // the simulation backend.
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => handle
                .spawn(async move { f() })
                .await
                .expect("simulation spawn_blocking task should not be cancelled"),
        }
    }

    pub async fn timeout<T>(
        &self,
        timeout_after: Duration,
        future: impl Future<Output = T>,
    ) -> Result<T, RuntimeTimeout> {
        match self {
            Self::Tokio(_) => tokio::time::timeout(timeout_after, future)
                .await
                .map_err(|_| RuntimeTimeout),
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => handle.timeout(timeout_after, future).await.map_err(|_| RuntimeTimeout),
        }
    }

    pub async fn sleep(&self, duration: Duration) {
        match self {
            Self::Tokio(_) => tokio::time::sleep(duration).await,
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => handle.sleep(duration).await,
        }
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        match self {
            Self::Tokio(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => handle.block_on(future),
        }
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[cfg(feature = "simulation")]
    #[test]
    fn dropping_joinhandle_does_not_cancel_task_in_simulation() {
        use crate::sim::Runtime;
        let mut rt = Runtime::new(4);
        let handle = Handle::simulation(rt.handle());
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        rt.block_on(async {
            let jh = handle.spawn(async move {
                flag_clone.store(true, Ordering::Release);
            });
            drop(jh);

            // Yield so the spawned task gets polled.
            handle
                .timeout(std::time::Duration::from_millis(50), async {})
                .await
                .ok();
        });

        assert!(flag.load(Ordering::Acquire));
    }

    #[cfg(feature = "simulation")]
    #[test]
    fn abort_cancels_task_in_simulation() {
        use crate::sim::Runtime;
        let mut rt = Runtime::new(4);
        let handle = Handle::simulation(rt.handle());
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();
        let handle_for_spawn = handle.clone();

        rt.block_on(async move {
            let jh = handle.spawn(async move {
                handle_for_spawn
                    .timeout(std::time::Duration::from_millis(100), async {})
                    .await
                    .ok();
                flag_clone.store(true, Ordering::Release);
            });
            jh.abort_handle().abort();

            let result = jh.await;
            let _ = handle.timeout(std::time::Duration::from_millis(500), async {}).await;
            assert!(result.is_err());
            assert!(!flag.load(Ordering::Acquire));
        });
    }
}
