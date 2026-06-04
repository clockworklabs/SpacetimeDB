use async_channel::SendError;

#[cfg(feature = "simulation")]
use async_channel::TrySendError;

/// Sending end of a bounded channel.
///
/// Production: uses `send_blocking` (futex, true backpressure).
/// Simulation: uses `try_send` + executor tick when full (no futex).
pub struct Sender<T> {
    inner: async_channel::Sender<T>,
    rt: crate::Handle,
}

/// Receiving end of a bounded channel.
///
/// Identical to `async_channel::Receiver` in both modes.
pub struct Receiver<T> {
    inner: async_channel::Receiver<T>,
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> async_channel::Recv<'_, T> {
        self.inner.recv()
    }

    pub fn try_recv(&self) -> Result<T, async_channel::TryRecvError> {
        self.inner.try_recv()
    }

    pub fn close(&self) {
        self.inner.close();
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Create a bounded channel with the given capacity and runtime handle.
///
/// The returned `Sender` adapts its send strategy based on the runtime variant:
/// - `Handle::Tokio` → `send_blocking` (OS thread park)
/// - `Handle::Simulation` → `try_send` + executor tick on full (no futex)
pub fn bounded<T>(cap: usize, rt: crate::Handle) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = async_channel::bounded(cap);
    (Sender { inner: tx, rt }, Receiver { inner: rx })
}

impl<T> Sender<T> {
    /// Close the sender, signalling that no more messages will be sent.
    pub fn close(&self) {
        self.inner.close();
    }

    /// Send a message, applying backpressure.
    ///
    /// In production (`Tokio`) this parks the OS thread via futex.
    /// In simulation it loops on `try_send`, calling `sim::Handle::run_all_ready()`
    /// when the channel is full so the actor can make progress without
    /// actually parking the (sole) thread.
    pub fn send_blocking(&self, msg: T) -> Result<(), SendError<T>> {
        match &self.rt {
            #[cfg(feature = "tokio")]
            crate::Handle::Tokio(_) => self.inner.send_blocking(msg),
            #[cfg(feature = "simulation")]
            crate::Handle::Simulation(sim) => {
                let mut msg = msg;
                loop {
                    match self.inner.try_send(msg) {
                        Ok(()) => return Ok(()),
                        Err(TrySendError::Full(m)) => {
                            msg = m;
                            sim.run_all_ready();
                        }
                        Err(TrySendError::Closed(m)) => return Err(SendError(m)),
                    }
                }
            }
            #[cfg(not(any(feature = "tokio", feature = "simulation")))]
            _ => unreachable!("runtime::channel::send called with no backend enabled"),
        }
    }
}
