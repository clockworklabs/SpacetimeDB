use crate::NodeDelegate;

#[cfg(not(target_env = "msvc"))]
mod jemalloc_profiling {
    use axum::body::Body;
    use axum::extract::Query;
    use axum::response::{IntoResponse, Response};
    use axum::Json;
    use http::header::CONTENT_TYPE;
    use http::StatusCode;
    use serde::{Deserialize, Serialize};

    /// Query parameters for the unified `heap` endpoint
    #[derive(Deserialize)]
    struct HeapQuery {
        format: Option<String>,
    }

    async fn handle_get_heap(Query(params): Query<HeapQuery>) -> Result<impl IntoResponse, (StatusCode, String)> {
        let Some(ctl) = jemalloc_pprof::PROF_CTL.as_ref() else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "jemalloc profiling is disabled and cannot be activated".into(),
            ));
        };

        let mut prof_ctl = ctl.lock().await;
        require_profiling_activated(&prof_ctl)?;

        match params.format.as_deref() {
            Some("flame") => {
                let svg = prof_ctl
                    .dump_flamegraph()
                    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
                Response::builder()
                    .header(CONTENT_TYPE, "image/svg+xml")
                    .body(Body::from(svg))
                    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }
            // Default to pprof if no or an invalid format is provided
            _ => {
                let pprof = prof_ctl
                    .dump_pprof()
                    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
                Response::builder()
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(Body::from(pprof))
                    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }
        }
    }

    /// Checks whether jemalloc profiling is activated an returns an error response if not.
    fn require_profiling_activated(prof_ctl: &jemalloc_pprof::JemallocProfCtl) -> Result<(), (StatusCode, String)> {
        if prof_ctl.activated() {
            Ok(())
        } else {
            Err((
                axum::http::StatusCode::FORBIDDEN,
                "heap profiling is not activate. Activate by POSTing to /heap/settings?enabled=true".into(),
            ))
        }
    }

    /// Query parameters for toggling heap profiling
    #[derive(Deserialize)]
    struct ToggleQuery {
        enabled: bool,
    }

    /// JSON response for the current state of heap profiling
    #[derive(Serialize)]
    struct CurrentState {
        enabled: bool,
    }

    /// Handles toggling heap profiling (on or off) via a `POST` request
    async fn handle_post_heap_enabled(
        Query(params): Query<ToggleQuery>,
    ) -> Result<impl IntoResponse, (StatusCode, String)> {
        let Some(ctl) = jemalloc_pprof::PROF_CTL.as_ref() else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "jemalloc profiling is disabled and cannot be activated".into(),
            ));
        };

        let mut prof_ctl = ctl.lock().await;

        if params.enabled {
            prof_ctl.activate().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to activate heap profiling: {}", e),
                )
            })?;
            Ok(("Heap profiling activated").into_response())
        } else {
            prof_ctl.deactivate().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to deactivate heap profiling: {}", e),
                )
            })?;
            Ok(("Heap profiling deactivated").into_response())
        }
    }

    /// Handles retrieving the current state of heap profiling via a `GET` request
    async fn handle_get_heap_enabled() -> Result<impl IntoResponse, (StatusCode, String)> {
        let Some(ctl) = jemalloc_pprof::PROF_CTL.as_ref() else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "jemalloc profiling is disabled and cannot be activated.".into(),
            ));
        };

        let prof_ctl = ctl.lock().await;

        let current_state = CurrentState {
            enabled: prof_ctl.activated(),
        };

        Ok(Json(current_state))
    }

    pub fn jemalloc_router<S: Clone + Send + Sync + 'static>() -> axum::Router<S> {
        use axum::routing::get;
        axum::Router::new()
            .route("/", get(handle_get_heap))
            .route("/settings", get(handle_get_heap_enabled).post(handle_post_heap_enabled))
    }
}

#[cfg(target_env = "msvc")]
mod jemalloc_profiling {
    use axum::response::IntoResponse;
    use http::StatusCode;

    async fn jemalloc_unsupported() -> impl IntoResponse {
        // Return an error for msvc environments
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "jemalloc heap profiling is not supported on this platform.",
        )
    }

    pub fn jemalloc_router<S: Clone + Send + Sync + 'static>() -> axum::Router<S> {
        use axum::routing::get;
        axum::Router::new()
            .route("/", get(jemalloc_unsupported))
            .route("/settings", get(jemalloc_unsupported).post(jemalloc_unsupported))
    }
}

// The internal router is for things that are not meant to be exposed to the public API.
pub fn router<S>() -> axum::Router<S>
where
    S: NodeDelegate + Clone + 'static,
{
    axum::Router::new().nest("/heap", jemalloc_profiling::jemalloc_router())
}
