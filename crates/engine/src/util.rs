use spacetimedb_runtime::Handle;
use tracing::Span;

/// Ergonomic wrapper for `runtime.spawn_blocking(f).await`.
///
/// If `f` panics, it will be bubbled up to the calling task.
pub async fn asyncify<F, R>(runtime: &Handle, f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    // Ensure that `f` executes in the current span context.
    // If there is no current span, or it is disabled, `span` is disabled.
    let span = Span::current();
    runtime
        .spawn_blocking(move || {
            let _enter = span.enter();
            f()
        })
        .await
}
