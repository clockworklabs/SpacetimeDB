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

#[cfg(feature = "tokio")]
pub type TokioHandle = tokio::runtime::Handle;

#[derive(Clone)]
pub enum Handle {
    #[cfg(feature = "tokio")]
    Tokio(TokioHandle),
    #[cfg(feature = "simulation")]
    Simulation(sim::Handle),
}

pub struct JoinHandle<T> {
    inner: JoinHandleInner<T>,
}

pub struct AbortHandle {
    inner: AbortHandleInner,
}

enum JoinHandleInner<T> {
    #[cfg(feature = "tokio")]
    Tokio(tokio::task::JoinHandle<T>),
    #[cfg(feature = "simulation")]
    Simulation(sim::JoinHandle<T>),
    Detached(PhantomData<T>),
}

enum AbortHandleInner {
    #[cfg(feature = "tokio")]
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
    #[cfg(feature = "tokio")]
    Tokio(tokio::task::JoinError),
    #[cfg(feature = "simulation")]
    Simulation(sim::JoinError),
}

impl AbortHandle {
    pub fn abort(&self) {
        match &self.inner {
            #[cfg(feature = "tokio")]
            AbortHandleInner::Tokio(handle) => handle.abort(),
            #[cfg(feature = "simulation")]
            AbortHandleInner::Simulation(handle) => handle.abort(),
            #[cfg(not(any(feature = "tokio", feature = "simulation")))]
            _ => unreachable!("runtime abort handle has no enabled backend"),
        }
    }
}

impl JoinErrorInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "tokio")]
            Self::Tokio(err) => fmt::Display::fmt(err, f),
            #[cfg(feature = "simulation")]
            Self::Simulation(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = f;
        #[cfg(any(feature = "tokio", feature = "simulation"))]
        return self.inner.fmt(f);
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        unreachable!("runtime join error has no enabled backend")
    }
}

#[cfg(any(feature = "tokio", feature = "simulation"))]
impl std::error::Error for JoinError {}

impl<T> JoinHandleInner<T> {
    fn abort_handle(&self) -> AbortHandle {
        match self {
            #[cfg(feature = "tokio")]
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
            #[cfg(feature = "tokio")]
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
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = cx;
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
        #[cfg(feature = "simulation")]
        if let JoinHandleInner::Simulation(handle) = inner {
            handle.detach();
            return;
        }
        // For Tokio (and Detached), dropping the handle does not cancel the task.
        drop(inner);
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

#[cfg(any(feature = "tokio", feature = "simulation"))]
impl std::error::Error for RuntimeTimeout {}

#[cfg(feature = "tokio")]
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
}

impl Handle {
    pub fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static) -> JoinHandle<T> {
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = future;
        match self {
            #[cfg(feature = "tokio")]
            Self::Tokio(handle) => JoinHandle {
                inner: JoinHandleInner::Tokio(handle.spawn(future)),
            },
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => JoinHandle {
                inner: JoinHandleInner::Simulation(handle.spawn_on(sim::NodeId::MAIN, future)),
            },
            #[cfg(not(any(feature = "tokio", feature = "simulation")))]
            _ => unreachable!("runtime dispatch has no enabled backend"),
        }
    }

    pub async fn spawn_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = &f;
        match self {
            #[cfg(feature = "tokio")]
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
                .spawn_on(sim::NodeId::MAIN, async move { f() })
                .await
                .expect("simulation spawn_blocking task should not be cancelled"),
            #[cfg(not(any(feature = "tokio", feature = "simulation")))]
            _ => unreachable!("runtime dispatch has no enabled backend"),
        }
    }

    pub async fn timeout<T>(
        &self,
        timeout_after: Duration,
        future: impl Future<Output = T>,
    ) -> Result<T, RuntimeTimeout> {
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = (timeout_after, future);
        match self {
            #[cfg(feature = "tokio")]
            Self::Tokio(_) => tokio::time::timeout(timeout_after, future)
                .await
                .map_err(|_| RuntimeTimeout),
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => handle.timeout(timeout_after, future).await.map_err(|_| RuntimeTimeout),
            #[cfg(not(any(feature = "tokio", feature = "simulation")))]
            _ => unreachable!("runtime dispatch has no enabled backend"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                // Sleep long enough that abort fires first.
                handle_for_spawn
                    .timeout(std::time::Duration::from_millis(100), async {})
                    .await
                    .ok();
                flag_clone.store(true, Ordering::Release);
            });
            jh.abort_handle().abort();

            let result = jh.await;
            // wait to see, above task indeed cancelled.
             let _ = handle
                    .timeout(std::time::Duration::from_millis(500), async {})
                    .await;
            assert!(result.is_err());
            assert!(!flag.load(Ordering::Acquire));
        });
    }
}
