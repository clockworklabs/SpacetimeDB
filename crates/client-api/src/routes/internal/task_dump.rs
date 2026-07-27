use axum::{extract::Extension, routing::get};

#[cfg(all(
    tokio_unstable,
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
))]
mod imp {
    use std::{collections::BTreeMap, fmt::Write as _, sync::Arc, time::Duration};

    use axum::{
        extract::{Extension, Query},
        response::Response,
    };
    use http::{header::CONTENT_TYPE, StatusCode};
    use serde::Deserialize;
    use tokio::{runtime::Handle, sync::Mutex};

    const MAX_TIMEOUT_MS: u64 = 30_000;

    #[derive(Clone)]
    struct Runtime {
        handle: Handle,
        dump_lock: Arc<Mutex<()>>,
    }

    /// The Tokio runtimes which can be inspected by the internal task dump endpoint.
    #[derive(Clone, Default)]
    pub struct TaskDumpRegistry {
        runtimes: Arc<BTreeMap<&'static str, Runtime>>,
    }

    impl TaskDumpRegistry {
        pub fn new(runtimes: impl IntoIterator<Item = (&'static str, Handle)>) -> Self {
            let runtimes = runtimes
                .into_iter()
                .map(|(name, handle)| {
                    (
                        name,
                        Runtime {
                            handle,
                            dump_lock: Arc::new(Mutex::new(())),
                        },
                    )
                })
                .collect();
            Self {
                runtimes: Arc::new(runtimes),
            }
        }

        fn get(&self, name: &str) -> Option<&Runtime> {
            self.runtimes.get(name)
        }

        fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
            self.runtimes.keys().copied()
        }
    }

    #[derive(Deserialize)]
    pub(super) struct TaskDumpQuery {
        runtime: String,
        timeout_ms: u64,
    }

    impl TaskDumpQuery {
        fn validate(&self) -> Result<(), (StatusCode, String)> {
            if !(1..=MAX_TIMEOUT_MS).contains(&self.timeout_ms) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("timeout_ms must be between 1 and {MAX_TIMEOUT_MS}"),
                ));
            }
            Ok(())
        }
    }

    pub(super) async fn handle_get_task_dump(
        Extension(registry): Extension<TaskDumpRegistry>,
        Query(query): Query<TaskDumpQuery>,
    ) -> Result<Response, (StatusCode, String)> {
        query.validate()?;

        let Some(runtime) = registry.get(&query.runtime).cloned() else {
            let valid = registry.names().collect::<Vec<_>>().join(", ");
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown Tokio runtime {:?}; valid runtimes: {valid}", query.runtime),
            ));
        };

        let permit = runtime.dump_lock.try_lock_owned().map_err(|_| {
            (
                StatusCode::CONFLICT,
                format!("a task dump is already in progress for runtime {:?}", query.runtime),
            )
        })?;

        // Keep the permit in the spawned task so that a timed-out dump continues
        // to exclude new requests until Tokio's dump future actually finishes.
        let dump_task = tokio::spawn(async move {
            let dump = runtime.handle.dump().await;
            (permit, dump)
        });
        let (_permit, dump) = tokio::time::timeout(Duration::from_millis(query.timeout_ms), dump_task)
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    format!(
                        "timed out after {}ms while dumping Tokio runtime {:?}",
                        query.timeout_ms, query.runtime
                    ),
                )
            })?
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("task dump worker failed for runtime {:?}: {err}", query.runtime),
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

        #[tokio::test]
        async fn registry_names_are_sorted() {
            let handle = Handle::current();
            let registry = TaskDumpRegistry::new([("replication", handle.clone()), ("main", handle)]);

            assert_eq!(registry.names().collect::<Vec<_>>(), ["main", "replication"]);
        }

        #[test]
        fn timeout_must_be_in_range() {
            for timeout_ms in [1, MAX_TIMEOUT_MS] {
                assert!(TaskDumpQuery {
                    runtime: "main".into(),
                    timeout_ms,
                }
                .validate()
                .is_ok());
            }

            for timeout_ms in [0, MAX_TIMEOUT_MS + 1] {
                assert!(TaskDumpQuery {
                    runtime: "main".into(),
                    timeout_ms,
                }
                .validate()
                .is_err());
            }
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn dumps_registered_runtime() {
            let registry = TaskDumpRegistry::new([("main", Handle::current())]);
            let response = handle_get_task_dump(
                Extension(registry),
                Query(TaskDumpQuery {
                    runtime: "main".into(),
                    timeout_ms: 10_000,
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

pub fn router<S: Clone + Send + Sync + 'static>(registry: TaskDumpRegistry) -> axum::Router<S> {
    axum::Router::new()
        .route("/", get(handle_get_task_dump))
        .layer(Extension(registry))
}
