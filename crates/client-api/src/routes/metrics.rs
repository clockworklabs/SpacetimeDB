use axum::body::Body;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use http::header::CONTENT_TYPE;

use crate::NodeDelegate;

// #[derive(Clone, NewMiddleware)]
// pub struct MetricsAuthMiddleware;

// impl Middleware for MetricsAuthMiddleware {
//     fn call<Chain>(self, state: State, chain: Chain) -> Pin<Box<HandlerFuture>>
//     where
//         Chain: FnOnce(State) -> Pin<Box<HandlerFuture>>,
//     {
//         chain(state)
//     }
// }

pub async fn metrics<S: NodeDelegate>(State(ctx): State<S>) -> axum::response::Result<impl IntoResponse> {
    let mut buf = String::new();

    let mut encode_to_buffer = |mfs: &[_]| {
        if let Err(e) = prometheus::TextEncoder.encode_utf8(mfs, &mut buf) {
            log::error!("could not encode custom metrics: {}", e);
        }
    };

    encode_to_buffer(&ctx.gather_metrics());
    encode_to_buffer(&prometheus::gather());

    Ok(buf)
}

use axum::http::StatusCode;

pub async fn handle_get_heap() -> Result<impl IntoResponse, (StatusCode, String)> {
    let Some(ctl) = jemalloc_pprof::PROF_CTL.as_ref() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "jemalloc profiling is disabled and cannot be activated".into(),
        ));
    };
    let mut prof_ctl = ctl.lock().await;
    require_profiling_activated(&prof_ctl)?;
    let pprof = prof_ctl
        .dump_pprof()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    // let svg = prof_ctl
    //     .dump_flamegraph()
    //     .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    // Response::builder()
    //     .header(CONTENT_TYPE, "image/svg+xml")
    //     .body(Body::from(svg))
    //     .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
    Ok(pprof)
}

/// Checks whether jemalloc profiling is activated an returns an error response if not.
fn require_profiling_activated(prof_ctl: &jemalloc_pprof::JemallocProfCtl) -> Result<(), (StatusCode, String)> {
    if prof_ctl.activated() {
        Ok(())
    } else {
        Err((axum::http::StatusCode::FORBIDDEN, "heap profiling not activated".into()))
    }
}

pub fn router<S>() -> axum::Router<S>
where
    S: NodeDelegate + Clone + 'static,
{
    use axum::routing::get;
    axum::Router::new()
        .route("/", get(metrics::<S>))
        .route("/heap", get(handle_get_heap))
    // TODO:
    // .layer(MetricsAuthMiddleware)
}
