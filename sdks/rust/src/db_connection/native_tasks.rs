//! Native connection tasks retained through terminal cleanup.

use crate::__codegen::InternalError;
use tokio::task::JoinHandle;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct NativeTasks {
    websocket: Option<JoinHandle<()>>,
    parser: Option<JoinHandle<()>>,
    failure: Option<crate::Error>,
}

impl NativeTasks {
    pub fn new(websocket: JoinHandle<()>, parser: JoinHandle<()>) -> Self {
        Self {
            websocket: Some(websocket),
            parser: Some(parser),
            failure: None,
        }
    }

    pub fn abort_handles(&self) -> Vec<tokio::task::AbortHandle> {
        [&self.websocket, &self.parser]
            .into_iter()
            .flatten()
            .map(JoinHandle::abort_handle)
            .collect()
    }

    pub fn record_failure(&mut self, error: crate::Error) {
        // A cancelled terminal wait must not discard its original failure.
        self.failure.get_or_insert(error);
    }

    /// Called only after event processing has terminated. In particular, a
    /// parser error must not leave the WebSocket waiting for more input.
    pub async fn stop_and_join(&mut self) -> crate::Result<()> {
        self.abort();
        // Each handle stays in shared connection state across await. Clear it
        // immediately after joining, before the next suspension point, so a
        // cancelled caller cannot detach it or poll a completed handle twice.
        Self::join(&mut self.websocket, &mut self.failure, "WebSocket").await;
        Self::join(&mut self.parser, &mut self.failure, "parser").await;
        self.failure.clone().map_or(Ok(()), Err)
    }

    async fn join(handle: &mut Option<JoinHandle<()>>, failure: &mut Option<crate::Error>, name: &str) {
        let Some(task) = handle.as_mut() else { return };
        let result = task.await;
        *handle = None;
        if let Err(error) = result {
            // Cancellation was requested above. A task panic is still a failure,
            // and repeated terminal waits must continue reporting that failure.
            if !error.is_cancelled() && failure.is_none() {
                *failure = Some(
                    InternalError::new(format!("Native {name} task failed"))
                        .with_cause(error)
                        .into(),
                );
            }
        }
    }

    fn abort(&self) {
        for task in [&self.websocket, &self.parser].into_iter().flatten() {
            task.abort();
        }
    }
}

impl Drop for NativeTasks {
    fn drop(&mut self) {
        // Dropping the last connection is a cancellation request, not a join.
        // Owners requiring completed shutdown must finish run_async/run_threaded.
        self.abort();
    }
}
