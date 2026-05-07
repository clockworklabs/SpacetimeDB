//! Runtime and deterministic simulation utilities shared by core and DST.

use std::{fmt, future::Future, time::Duration};

#[cfg(feature = "simulation")]
pub mod sim;

#[cfg(feature = "tokio")]
pub type Handle = tokio::runtime::Handle;
#[cfg(feature = "tokio")]
pub type Runtime = tokio::runtime::Runtime;

#[derive(Clone)]
pub enum RuntimeDispatch {
    #[cfg(feature = "tokio")]
    Tokio(Handle),
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

impl std::error::Error for RuntimeTimeout {}

impl RuntimeDispatch {
    #[cfg(feature = "tokio")]
    pub fn tokio(handle: Handle) -> Self {
        Self::Tokio(handle)
    }

    #[cfg(feature = "tokio")]
    pub fn tokio_current() -> Self {
        Self::tokio(Handle::current())
    }

    #[cfg(feature = "simulation")]
    pub fn simulation(handle: sim::Handle) -> Self {
        Self::Simulation(handle)
    }

    #[cfg(feature = "simulation")]
    pub fn simulation_current() -> Self {
        Self::simulation(sim::Handle::current().expect("simulation runtime is not active on this thread"))
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
            Self::Simulation(_) => sim::time::timeout(timeout_after, future)
                .await
                .map_err(|_| RuntimeTimeout),
            #[cfg(not(any(feature = "tokio", feature = "simulation")))]
            _ => unreachable!("runtime dispatch has no enabled backend"),
        }
    }
}

#[cfg(feature = "tokio")]
pub fn current_handle_or_new_runtime() -> anyhow::Result<(Handle, Option<Runtime>)> {
    if let Ok(handle) = Handle::try_current() {
        return Ok((handle, None));
    }

    let runtime = Runtime::new()?;
    Ok((runtime.handle().clone(), Some(runtime)))
}
