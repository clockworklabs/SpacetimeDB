use axum::extract::{FromRef, State};
use axum::response::IntoResponse;
use std::sync::Arc;

use crate::{ControlNodeDelegate, WorkerCtx};

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

pub async fn metrics(State(ctx): State<Arc<dyn WorkerCtx>>) -> axum::response::Result<impl IntoResponse> {
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

pub fn router<S>() -> axum::Router<S>
where
    S: ControlNodeDelegate + Clone + 'static,
    Arc<dyn WorkerCtx>: FromRef<S>,
{
    use axum::routing::get;
    axum::Router::new().route("/", get(metrics))
    // TODO:
    // .layer(MetricsAuthMiddleware)
}
