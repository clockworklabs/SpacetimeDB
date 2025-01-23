use axum::extract::State;
use axum::response::IntoResponse;

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
    println!("metrics");
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
    S: NodeDelegate + Clone + 'static,
{
    use axum::routing::get;
    axum::Router::new().route("/", get(metrics::<S>))
    // TODO:
    // .layer(MetricsAuthMiddleware)
}
