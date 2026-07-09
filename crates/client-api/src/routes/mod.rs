use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::{Body, Bytes, HttpBody};
use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use http::header;
use http_body::Frame;
use ::prometheus::IntCounter;
use spacetimedb::worker_metrics::WORKER_METRICS;
use tower_http::cors;

use crate::{Authorization, ControlStateDelegate, NodeDelegate};

pub mod database;
pub mod energy;
pub mod health;
pub mod identity;
mod internal;
pub mod metrics;
pub mod prometheus;
pub mod subscribe;

use self::{database::DatabaseRoutes, identity::IdentityRoutes};

/// This API call is just designed to allow clients to determine whether or not they can
/// establish a connection to SpacetimeDB. This API call doesn't actually do anything.
pub async fn ping(_auth: crate::auth::SpacetimeAuthHeader) {}

/// A body wrapper that counts bytes as they are actually transferred.
struct CountingBody {
    inner: Body,
    counter: IntCounter,
}

impl HttpBody for CountingBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let poll = Pin::new(&mut self.inner).poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &poll {
            if let Some(data) = frame.data_ref() {
                self.counter.inc_by(data.len() as u64);
            }
        }
        poll
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
}

impl CountingBody {
    fn wrap(counter: IntCounter) -> impl FnOnce(Body) -> Body {
        move |inner| Body::new(CountingBody { inner, counter })
    }
}

/// Returns the method as a static label value,
/// bucketing non-standard methods to keep label cardinality bounded.
fn method_label(method: &http::Method) -> &'static str {
    match method.as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DELETE",
        "HEAD" => "HEAD",
        "OPTIONS" => "OPTIONS",
        "PATCH" => "PATCH",
        "CONNECT" => "CONNECT",
        "TRACE" => "TRACE",
        _ => "OTHER",
    }
}

/// Records request count, latency and body sizes per HTTP route.
async fn http_metrics_middleware(req: Request, next: Next) -> Response {
    let Some(route) = req.extensions().get::<MatchedPath>().cloned() else {
        return next.run(req).await;
    };

    let method = method_label(req.method());

    let request_body_bytes = WORKER_METRICS.http_request_body_bytes.with_label_values(route.as_str());
    let response_body_bytes = WORKER_METRICS.http_response_body_bytes.with_label_values(route.as_str());

    let req = req.map(CountingBody::wrap(request_body_bytes));

    let start = std::time::Instant::now();
    let res = next.run(req).await;

    WORKER_METRICS
        .http_requests
        .with_label_values(route.as_str(), method, res.status().as_str())
        .inc();
    WORKER_METRICS
        .http_request_duration
        .with_label_values(route.as_str())
        .observe(start.elapsed().as_secs_f64());

    res.map(CountingBody::wrap(response_body_bytes))
}

#[allow(clippy::let_and_return)]
pub fn router<S>(
    ctx: &S,
    database_routes: DatabaseRoutes<S>,
    identity_routes: IdentityRoutes<S>,
    extra: axum::Router<S>,
) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Authorization + Clone + 'static,
{
    use axum::routing::get;
    let router = axum::Router::new()
        .nest("/database", database_routes.into_router(ctx.clone()))
        .nest("/identity", identity_routes.into_router())
        .nest("/energy", energy::router())
        .nest("/prometheus", prometheus::router())
        .nest("/metrics", metrics::router())
        .route("/ping", get(ping))
        .merge(extra);

    let cors = cors::CorsLayer::new()
        .allow_headers([header::AUTHORIZATION, header::ACCEPT, header::CONTENT_TYPE])
        .allow_methods(cors::Any)
        .allow_origin(cors::Any);

    axum::Router::new()
        .nest(
            "/v1",
            router
                .layer(axum::middleware::from_fn(http_metrics_middleware))
                .layer(cors),
        )
        .nest("/internal", internal::router())
}
