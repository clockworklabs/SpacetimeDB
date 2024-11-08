use futures::{Future, FutureExt};
use std::borrow::Cow;
use std::pin::pin;
use tokio::sync::oneshot;

pub mod prometheus_handle;

pub mod lending_pool;
pub mod notify_once;
pub mod slow;

pub(crate) fn string_from_utf8_lossy_owned(v: Vec<u8>) -> String {
    match String::from_utf8_lossy(&v) {
        // SAFETY: from_utf8_lossy() returned Borrowed, which means the original buffer is valid utf8
        Cow::Borrowed(_) => unsafe { String::from_utf8_unchecked(v) },
        Cow::Owned(s) => s,
    }
}

#[track_caller]
pub const fn const_unwrap<T: Copy>(o: Option<T>) -> T {
    match o {
        Some(x) => x,
        None => panic!("called `const_unwrap()` on a `None` value"),
    }
}

#[tracing::instrument(skip_all)]
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
