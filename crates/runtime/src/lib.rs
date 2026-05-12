#![cfg_attr(not(any(feature = "tokio", feature = "simulation-std")), no_std)]

//! Runtime and deterministic simulation utilities shared by core and DST.

extern crate alloc;

use core::{fmt, future::Future, time::Duration};

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

    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        #[cfg(not(any(feature = "tokio", feature = "simulation")))]
        let _ = future;
        match self {
            #[cfg(feature = "tokio")]
            Self::Tokio(handle) => {
                handle.spawn(future);
            }
            #[cfg(feature = "simulation")]
            Self::Simulation(handle) => {
                handle.spawn_on(sim::NodeId::MAIN, future).detach();
            }
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
