use futures::{Future, FutureExt};
use std::borrow::Cow;
use std::pin::pin;
use tokio::sync::oneshot;
use tracing::Span;

pub mod prometheus_handle;

pub mod jobs;
pub mod notify_once;
pub mod slow;

// TODO: use String::from_utf8_lossy_owned once stabilized
pub(crate) fn string_from_utf8_lossy_owned(v: Vec<u8>) -> String {
    match String::from_utf8_lossy(&v) {
        // SAFETY: from_utf8_lossy() returned Borrowed, which means the original buffer is valid utf8
        Cow::Borrowed(_) => unsafe { String::from_utf8_unchecked(v) },
        Cow::Owned(s) => s,
    }
}

#[tracing::instrument(level = "trace", skip_all)]
pub fn spawn_rayon<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> impl Future<Output = R> {
    let span = tracing::Span::current();
    let (tx, rx) = oneshot::channel();
    rayon::spawn(|| {
        let _entered = span.entered();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if let Err(Err(_panic)) = tx.send(result) {
            tracing::warn!("uncaught panic on threadpool")
        }
    });
    rx.map(|res| res.unwrap().unwrap_or_else(|err| std::panic::resume_unwind(err)))
}

/// Ergonomic wrapper for `tokio::task::spawn_blocking(f).await`.
///
/// If `f` panics, it will be bubbled up to the calling task.
pub async fn asyncify<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    // Ensure that `f` executes in the current span context.
    // If there is no current span, or it is disabled, `span` is disabled.
    let span = Span::current();
    tokio::task::spawn_blocking(move || {
        let _enter = span.enter();
        f()
    })
    .await
    .unwrap_or_else(|e| match e.try_into_panic() {
        Ok(panic_payload) => std::panic::resume_unwind(panic_payload),
        // the only other variant is cancelled, which shouldn't happen because we don't cancel it.
        Err(e) => panic!("Unexpected JoinError: {e}"),
    })
}

/// Await `fut`, while also polling `also`.
pub async fn also_poll<Fut: Future>(fut: Fut, also: impl Future<Output = ()>) -> Fut::Output {
    let mut also = pin!(also.fuse());
    let mut fut = pin!(fut);
    std::future::poll_fn(|cx| {
        let _ = also.poll_unpin(cx);
        fut.poll_unpin(cx)
    })
    .await
}
