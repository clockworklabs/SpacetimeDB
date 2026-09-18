//! Physical operation ownership, independent of an HTTP/WebSocket waiter's lifetime.
use super::NoSuchModule;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::{Notify, OwnedSemaphorePermit};

#[derive(Default)]
pub(super) struct ModuleOperations {
    state: Mutex<State>,
    drained: Notify,
}

#[derive(Default)]
struct State {
    closed: bool,
    active: usize,
}

impl ModuleOperations {
    pub(super) fn begin(self: &Arc<Self>) -> Result<OperationLease, NoSuchModule> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(NoSuchModule);
        }
        state.active = state.active.checked_add(1).ok_or(NoSuchModule)?;
        Ok(OperationLease {
            _token: Arc::new(ActiveOperation(self.clone())),
            pool_slot: None,
        })
    }

    #[cfg(test)]
    pub(super) fn active(&self) -> usize {
        self.state.lock().active
    }

    #[cfg(test)]
    pub(super) fn is_closed(&self) -> bool {
        self.state.lock().closed
    }

    pub(super) fn close(&self) {
        self.state.lock().closed = true;
    }

    pub(super) async fn drained(&self) {
        loop {
            let notified = self.drained.notified();
            tokio::pin!(notified);
            // Register before reading the counter, including on a multi-threaded runtime.
            notified.as_mut().enable();
            if self.state.lock().active == 0 {
                return;
            }
            notified.await;
        }
    }
}

/// Clones describe one admitted operation, not additional admissions. A clone
/// moves into the physical job/request before it is enqueued. Cancellation of
/// the caller therefore cannot release admission or a pooled instance's slot.
#[derive(Clone)]
pub(in crate::host) struct OperationLease {
    _token: Arc<ActiveOperation>,
    pool_slot: Option<Arc<OwnedSemaphorePermit>>,
}

impl OperationLease {
    pub(super) fn with_pool_slot(mut self, slot: Option<Arc<OwnedSemaphorePermit>>) -> Self {
        self.pool_slot = slot;
        self
    }
}

struct ActiveOperation(Arc<ModuleOperations>);

impl Drop for ActiveOperation {
    fn drop(&mut self) {
        let mut state = self.0.state.lock();
        state.active -= 1;
        if state.active == 0 {
            self.0.drained.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn close_counts_previously_admitted_work_even_before_enqueue() {
        let operations = Arc::new(ModuleOperations::default());
        let operation = operations.begin().unwrap();
        operations.close();
        assert!(operations.begin().is_err());
        let waiter = tokio::spawn({
            let operations = operations.clone();
            async move { operations.drained().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        drop(operation);
        waiter.await.unwrap();
        operations.drained().await;
    }

    #[tokio::test]
    async fn cancelled_waiter_does_not_release_physical_job_or_pool_slot() {
        let operations = Arc::new(ModuleOperations::default());
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let caller = operations
            .begin()
            .unwrap()
            .with_pool_slot(Some(Arc::new(slots.clone().acquire_owned().await.unwrap())));
        let physical_job = caller.clone();
        drop(caller);
        operations.close();
        assert_eq!(slots.available_permits(), 0);
        assert_eq!(operations.state.lock().active, 1);
        drop(physical_job);
        operations.drained().await;
        assert_eq!(slots.available_permits(), 1);
    }
}
