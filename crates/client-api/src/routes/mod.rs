use axum::body::HttpBody as _;
use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use http::header;
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

/// Records request count, latency and body sizes per HTTP route.
async fn http_metrics_middleware(req: Request, next: Next) -> Response {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "<unmatched>".to_owned(), |p| p.as_str().to_owned());
    let method = req.method().as_str().to_owned();

    let request_body_bytes = req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let start = std::time::Instant::now();
    let res = next.run(req).await;

    WORKER_METRICS
        .http_requests
        .with_label_values(&route, &method, res.status().as_str())
        .inc();
    WORKER_METRICS
        .http_request_duration
        .with_label_values(&route)
        .observe(start.elapsed().as_secs_f64());
    if let Some(n) = request_body_bytes {
        WORKER_METRICS.http_request_body_bytes.with_label_values(&route).inc_by(n);
    }
    if let Some(n) = res.body().size_hint().exact() {
        WORKER_METRICS
            .http_response_body_bytes
            .with_label_values(&route)
            .inc_by(n);
    }

    res
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
