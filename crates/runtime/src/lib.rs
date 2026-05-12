#![cfg_attr(not(any(feature = "tokio", feature = "simulation-std")), no_std)]

//! Runtime and deterministic simulation utilities shared by core and DST.

extern crate alloc;

use core::{
    fmt,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

pub mod adapter;
#[cfg(feature = "simulation")]
pub mod sim;

#[cfg(feature = "tokio")]
pub use adapter::tokio::{current_handle_or_new_runtime, TokioHandle, TokioRuntime};

#[derive(Clone)]
pub enum Runtime {
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
    Tokio(Option<tokio::task::JoinHandle<T>>),
    #[cfg(feature = "simulation")]
    Simulation(Option<sim::JoinHandle<T>>),
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

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = f;
        match &self.inner {
            #[cfg(feature = "tokio")]
            JoinErrorInner::Tokio(err) => err.fmt(f),
            #[cfg(feature = "simulation")]
            JoinErrorInner::Simulation(err) => err.fmt(f),
            #[cfg(not(any(feature = "tokio", feature = "simulation")))]
            _ => unreachable!("runtime join error has no enabled backend"),
        }
    }
}

#[cfg(any(feature = "tokio", feature = "simulation-std"))]
impl std::error::Error for JoinError {}

impl<T> JoinHandle<T> {
    pub fn abort_handle(&self) -> AbortHandle {
        match &self.inner {
            #[cfg(feature = "tokio")]
            JoinHandleInner::Tokio(Some(handle)) => AbortHandle {
                inner: AbortHandleInner::Tokio(handle.abort_handle()),
            },
            #[cfg(feature = "simulation")]
            JoinHandleInner::Simulation(Some(handle)) => AbortHandle {
                inner: AbortHandleInner::Simulation(handle.abort_handle()),
            },
            #[cfg(feature = "tokio")]
            JoinHandleInner::Tokio(None) => panic!("runtime join handle aborted after detach"),
            #[cfg(feature = "simulation")]
            JoinHandleInner::Simulation(None) => panic!("runtime join handle aborted after detach"),
            JoinHandleInner::Detached(_) => panic!("runtime join handle aborted after completion"),
        }
    }

    pub fn detach(mut self) {
        self.detach_inner();
    }

    fn detach_inner(&mut self) {
        match &mut self.inner {
            #[cfg(feature = "tokio")]
            JoinHandleInner::Tokio(handle) => {
                drop(handle.take());
            }
            #[cfg(feature = "simulation")]
            JoinHandleInner::Simulation(handle) => {
                if let Some(handle) = handle.take() {
                    handle.detach();
                }
            }
            JoinHandleInner::Detached(_) => {}
        }
        self.inner = JoinHandleInner::Detached(PhantomData);
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = cx;
        match &mut self.inner {
            #[cfg(feature = "tokio")]
            JoinHandleInner::Tokio(Some(handle)) => match Pin::new(handle).poll(cx) {
                Poll::Ready(Ok(output)) => {
                    self.inner = JoinHandleInner::Detached(PhantomData);
                    Poll::Ready(Ok(output))
                }
                Poll::Ready(Err(err)) => Poll::Ready(Err(JoinError {
                    inner: JoinErrorInner::Tokio(err),
                })),
                Poll::Pending => Poll::Pending,
            },
            #[cfg(feature = "simulation")]
            JoinHandleInner::Simulation(Some(handle)) => match Pin::new(handle).poll_join(cx) {
                Poll::Ready(Ok(output)) => {
                    self.inner = JoinHandleInner::Detached(PhantomData);
                    Poll::Ready(Ok(output))
                }
                Poll::Ready(Err(err)) => Poll::Ready(Err(JoinError {
                    inner: JoinErrorInner::Simulation(err),
                })),
                Poll::Pending => Poll::Pending,
            },
            #[cfg(feature = "tokio")]
            JoinHandleInner::Tokio(None) => panic!("runtime join handle polled after detach"),
            #[cfg(feature = "simulation")]
            JoinHandleInner::Simulation(None) => panic!("runtime join handle polled after detach"),
            JoinHandleInner::Detached(_) => panic!("runtime join handle polled after completion"),
        }
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        self.detach_inner();
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

#[cfg(any(feature = "tokio", feature = "simulation-std"))]
impl std::error::Error for RuntimeTimeout {}

impl Runtime {
    #[cfg(feature = "tokio")]
    pub fn tokio(handle: TokioHandle) -> Self {
        Self::Tokio(handle)
    }

    #[cfg(feature = "tokio")]
    pub fn tokio_current() -> Self {
        Self::tokio(TokioHandle::current())
    }

    #[cfg(feature = "simulation")]
    pub fn simulation(handle: sim::Handle) -> Self {
        Self::Simulation(handle)
    }

    #[cfg(feature = "simulation-std")]
    pub fn simulation_current() -> Self {
        adapter::sim_std::simulation_current()
    }

    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) -> JoinHandle<()> {
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = future;
        match self {
            #[cfg(feature = "tokio")]
            Self::Tokio(handle) => JoinHandle {
                inner: JoinHandleInner::Tokio(Some(handle.spawn(future))),
            },
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => JoinHandle {
                inner: JoinHandleInner::Simulation(Some(handle.spawn_on(sim::NodeId::MAIN, future))),
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
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => handle.spawn_on(sim::NodeId::MAIN, async move { f() }).await,
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
