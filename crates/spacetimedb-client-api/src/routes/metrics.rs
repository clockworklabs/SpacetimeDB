use gotham::handler::{HandlerFuture, SimpleHandlerResult};
use gotham::middleware::Middleware;
use gotham::pipeline::new_pipeline;
use gotham::pipeline::single_pipeline;
use gotham::router::builder::*;
use gotham::router::Router;
use gotham::state::State;
use gotham_derive::NewMiddleware;
use hyper::{Body, Response, StatusCode};
use std::pin::Pin;
use std::sync::Arc;

use crate::ApiCtx;

#[derive(Clone, NewMiddleware)]
pub struct MetricsAuthMiddleware;

impl Middleware for MetricsAuthMiddleware {
    fn call<Chain>(self, state: State, chain: Chain) -> Pin<Box<HandlerFuture>>
    where
        Chain: FnOnce(State) -> Pin<Box<HandlerFuture>>,
    {
        chain(state)
    }
}

async fn metrics(ctx: &dyn ApiCtx, _state: &mut State) -> SimpleHandlerResult {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();

    let mut buffer = Vec::new();

    if let Err(e) = encoder.encode(&ctx.gather_metrics(), &mut buffer) {
        log::error!("could not encode custom metrics: {}", e);
    };
    let mut res = match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            log::error!("custom metrics could not be from_utf8'd: {}", e);
            String::default()
        }
    };
    buffer.clear();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        log::error!("could not encode prometheus metrics: {}", e);
    };
    let res_custom = match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            log::error!("prometheus metrics could not be from_utf8'd: {}", e);
            String::default()
        }
    };
    buffer.clear();

    res.push_str(&res_custom);
    let body = Body::from(res);
    let ok = Response::builder().status(StatusCode::OK).body(body).unwrap();
    Ok(ok)
}

pub fn router(ctx: &Arc<dyn ApiCtx>) -> Router {
    let (admin_chain, admin) = single_pipeline(new_pipeline().add(MetricsAuthMiddleware).build());
    build_router(admin_chain, admin, |route| {
        route.get("/").to_new_handler(with_ctx!(ctx, metrics));
    })
}
