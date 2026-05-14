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
            Self::Tokio(Some(handle)) => AbortHandle {
                inner: AbortHandleInner::Tokio(handle.abort_handle()),
            },
            #[cfg(feature = "simulation")]
            Self::Simulation(Some(handle)) => AbortHandle {
                inner: AbortHandleInner::Simulation(handle.abort_handle()),
            },
            #[cfg(feature = "tokio")]
            Self::Tokio(None) => panic!("runtime join handle aborted after detach"),
            #[cfg(feature = "simulation")]
            Self::Simulation(None) => panic!("runtime join handle aborted after detach"),
            Self::Detached(_) => panic!("runtime join handle aborted after completion"),
        }
    }

    fn detach(&mut self) {
        match self {
            #[cfg(feature = "tokio")]
            Self::Tokio(handle) => {
                drop(handle.take());
            }
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => {
                if let Some(handle) = handle.take() {
                    handle.detach();
                }
            }
            Self::Detached(_) => {}
        }
    }

    fn poll_result(&mut self, cx: &mut Context<'_>) -> Poll<Result<T, JoinError>> {
        match self {
            #[cfg(feature = "tokio")]
            Self::Tokio(Some(handle)) => match Pin::new(handle).poll(cx) {
                Poll::Ready(Ok(output)) => Poll::Ready(Ok(output)),
                Poll::Ready(Err(err)) => Poll::Ready(Err(JoinError {
                    inner: JoinErrorInner::Tokio(err),
                })),
                Poll::Pending => Poll::Pending,
            },
            #[cfg(feature = "simulation")]
            Self::Simulation(Some(handle)) => match Pin::new(handle).poll_join(cx) {
                Poll::Ready(Ok(output)) => Poll::Ready(Ok(output)),
                Poll::Ready(Err(err)) => Poll::Ready(Err(JoinError {
                    inner: JoinErrorInner::Simulation(err),
                })),
                Poll::Pending => Poll::Pending,
            },
            #[cfg(feature = "tokio")]
            Self::Tokio(None) => panic!("runtime join handle polled after detach"),
            #[cfg(feature = "simulation")]
            Self::Simulation(None) => panic!("runtime join handle polled after detach"),
            Self::Detached(_) => panic!("runtime join handle polled after completion"),
        }
    }
}

impl<T> JoinHandle<T> {
    pub fn abort_handle(&self) -> AbortHandle {
        self.inner.abort_handle()
    }

    pub fn detach(mut self) {
        self.detach_inner();
    }

    fn detach_inner(&mut self) {
        self.inner.detach();
        self.inner = JoinHandleInner::Detached(PhantomData);
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
