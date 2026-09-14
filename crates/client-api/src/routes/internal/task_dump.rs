use axum::routing::get;

#[cfg(all(
    tokio_unstable,
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
))]
mod imp {
    use std::{collections::BTreeMap, fmt::Write as _, num::NonZeroU64, sync::Arc, time::Duration};

    use axum::{
        extract::{Extension, Query},
        response::Response,
    };
    use http::{header::CONTENT_TYPE, StatusCode};
    use serde::Deserialize;
    use tokio::runtime::Handle;

    const DEFAULT_TIMEOUT_MS: u64 = 2_000;

    /// The Tokio runtimes which can be inspected by the internal task dump endpoint.
    #[derive(Clone, Default)]
    pub struct TaskDumpRegistry {
        runtimes: Arc<BTreeMap<&'static str, Handle>>,
    }

    impl TaskDumpRegistry {
        pub fn new(runtimes: impl IntoIterator<Item = (&'static str, Handle)>) -> Self {
            Self {
                runtimes: Arc::new(runtimes.into_iter().collect()),
            }
        }
    }

    #[derive(Deserialize)]
    pub(super) struct TaskDumpQuery {
        runtime: String,
        timeout_ms: Option<NonZeroU64>,
    }

    pub(super) async fn handle_get_task_dump(
        registry: Option<Extension<TaskDumpRegistry>>,
        Query(query): Query<TaskDumpQuery>,
    ) -> Result<Response, (StatusCode, String)> {
        let Some(Extension(registry)) = registry else {
            return Err((
                StatusCode::NOT_IMPLEMENTED,
                "Tokio task dumps are not configured for this server".into(),
            ));
        };

        let Some(runtime) = registry.runtimes.get(query.runtime.as_str()).cloned() else {
            let valid = registry.runtimes.keys().copied().collect::<Vec<_>>().join(", ");
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown Tokio runtime {:?}; valid runtimes: {valid}", query.runtime),
            ));
        };

        let timeout_ms = query.timeout_ms.map_or(DEFAULT_TIMEOUT_MS, NonZeroU64::get);
        let dump = tokio::time::timeout(Duration::from_millis(timeout_ms), runtime.dump())
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    format!(
                        "timed out after {timeout_ms}ms while dumping Tokio runtime {:?}",
                        query.runtime
                    ),
                )
            })?;

        let runtime_name = query.runtime;
        let body = tokio::task::spawn_blocking(move || format_dump(&runtime_name, dump))
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("task dump formatting failed: {err}"),
                )
            })?;

        Response::builder()
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(body.into())
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
    }

    fn format_dump(runtime_name: &str, dump: tokio::runtime::Dump) -> String {
        let tasks = dump.tasks();
        let mut output = String::new();
        writeln!(output, "runtime: {runtime_name}").unwrap();
        writeln!(output, "tasks: {}", tasks.iter().count()).unwrap();

        for task in tasks.iter() {
            writeln!(output, "\nTASK {}:", task.id()).unwrap();
            writeln!(output, "{}", task.trace()).unwrap();
        }

        output
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use http_body_util::BodyExt as _;
        use tokio::runtime::Handle;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn dumps_registered_runtime_with_default_timeout() {
            let registry = TaskDumpRegistry::new([("main", Handle::current())]);
            let response = handle_get_task_dump(
                Some(Extension(registry)),
                Query(TaskDumpQuery {
                    runtime: "main".into(),
                    timeout_ms: None,
                }),
            )
            .await
            .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let body = std::str::from_utf8(&body).unwrap();
            assert!(body.starts_with("runtime: main\ntasks: "));
        }
    }
}

#[cfg(not(all(
    tokio_unstable,
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
)))]
mod imp {
    use axum::response::{IntoResponse as _, Response};
    use http::StatusCode;
    use tokio::runtime::Handle;

    /// The Tokio runtimes which can be inspected by the internal task dump endpoint.
    #[derive(Clone, Default)]
    pub struct TaskDumpRegistry;

    impl TaskDumpRegistry {
        pub fn new(_: impl IntoIterator<Item = (&'static str, Handle)>) -> Self {
            Self
        }
    }

    pub(super) async fn handle_get_task_dump() -> Response {
        (
            StatusCode::NOT_IMPLEMENTED,
            "Tokio task dumps require a Linux aarch64, x86, or x86_64 build with tokio_unstable enabled",
        )
            .into_response()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn reports_unsupported_platform() {
            assert_eq!(handle_get_task_dump().await.status(), StatusCode::NOT_IMPLEMENTED);
        }
    }
}

use imp::handle_get_task_dump;
pub use imp::TaskDumpRegistry;

pub fn router<S: Clone + Send + Sync + 'static>() -> axum::Router<S> {
    axum::Router::new().route("/", get(handle_get_task_dump))
}
