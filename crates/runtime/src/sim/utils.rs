use alloc::vec::Vec;
use alloc::sync::Arc;

use spin::Mutex;

use super::Rng;

/// Shared queue used by the simulation executor.
pub struct Queue<T> {
    inner: Arc<QueueInner<T>>,
}

/// Sending end of a queue.
pub struct Sender<T> {
    inner: Arc<QueueInner<T>>,
}

/// Receiving end of a queue.
pub struct Receiver<T> {
    inner: Arc<QueueInner<T>>,
}

// Manual Clone impls avoid the `T: Clone` bound that `derive` adds.
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

/// Queue storage.
struct QueueInner<T> {
    queue: Mutex<Vec<T>>,
}

impl<T> Queue<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(QueueInner {
                queue: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn sender(&self) -> Sender<T> {
        Sender {
            inner: self.inner.clone(),
        }
    }

    pub fn receiver(&self) -> Receiver<T> {
        Receiver {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Sender<T> {
    /// Push a value onto the shared queue.
    pub fn send(&self, value: T) {
        self.inner.queue.lock().push(value);
    }
}

impl<T> Receiver<T> {
    /// Remove one value using the runtime RNG to choose among ready items.
    pub fn try_recv_random(&self, rng: &Rng) -> Option<T> {
        let mut queue = self.inner.queue.lock();
        if queue.is_empty() {
            return None;
        }
        let idx = rng.index(queue.len());
        Some(queue.swap_remove(idx))
    }
}
