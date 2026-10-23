use axum::routing::MethodRouter;
use headers::Header;
use http::header;
use spacetimedb_client_api_messages::publish::{SpacetimeEnvironment, SpacetimeEnvironmentRemove};
use tower_http::cors;

use crate::{Authorization, ControlStateDelegate, NodeDelegate};

pub mod database;
pub mod energy;
pub mod health;
pub mod identity;
mod internal;
pub mod mcp;
pub mod metrics;
pub mod prometheus;
pub mod subscribe;

pub use self::internal::TaskDumpRegistry;
use self::{database::DatabaseRoutes, identity::IdentityRoutes};

/// This API call is just designed to allow clients to determine whether or not they can
/// establish a connection to SpacetimeDB. This API call doesn't actually do anything.
pub async fn ping(_auth: crate::auth::SpacetimeAuthHeader) {}

/// Allows the edition to customize the routes directly under `/v1`, as [`DatabaseRoutes`] does for `/database`.
pub struct RootRoutes<S> {
    /// GET: /ping
    pub ping_get: MethodRouter<S>,
    /// POST: /mcp
    pub mcp_post: MethodRouter<S>,
}

impl<S> Default for RootRoutes<S>
where
    S: NodeDelegate + ControlStateDelegate + Authorization + Clone + 'static,
{
    fn default() -> Self {
        use axum::routing::{get, post};
        Self {
            ping_get: get(ping),
            mcp_post: post(mcp::mcp_root::<S>),
        }
    }
}

pub fn router<S>(
    ctx: &S,
    database_routes: DatabaseRoutes<S>,
    identity_routes: IdentityRoutes<S>,
    extra: axum::Router<S>,
) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Authorization + Clone + 'static,
{
    router_with_root_routes(ctx, database_routes, identity_routes, RootRoutes::default(), extra)
}

pub fn router_with_root_routes<S>(
    ctx: &S,
    database_routes: DatabaseRoutes<S>,
    identity_routes: IdentityRoutes<S>,
    root_routes: RootRoutes<S>,
    extra: axum::Router<S>,
) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Authorization + Clone + 'static,
{
    let router = axum::Router::new()
        .nest("/database", database_routes.into_router(ctx.clone()))
        .nest("/identity", identity_routes.into_router())
        .nest("/energy", energy::router())
        .nest("/prometheus", prometheus::router())
        .nest("/metrics", metrics::router())
        // the database is named in the request body, so `mcp_root` counts its own egress
        .route(
            "/mcp",
            root_routes.mcp_post.route_layer(axum::middleware::from_fn_with_state(
                ctx.clone(),
                crate::auth::anon_auth_middleware::<S>,
            )),
        )
        .route("/ping", root_routes.ping_get)
        .merge(extra);

    axum::Router::new()
        .nest("/v1", router.layer(cors_layer()))
        .nest("/internal", internal::router())
}

/// Browsers may call the `/v1` API from any origin. Requests authenticate with an explicit
/// bearer token rather than cookies, so every header the API reads must be allowed here.
fn cors_layer() -> cors::CorsLayer {
    cors::CorsLayer::new()
        .allow_headers([
            header::AUTHORIZATION,
            header::ACCEPT,
            header::CONTENT_TYPE,
            SpacetimeEnvironment::name().clone(),
            SpacetimeEnvironmentRemove::name().clone(),
        ])
        .allow_methods(cors::Any)
        .allow_origin(cors::Any)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::{Method, Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn cors_preflight_allows_environment_headers() {
        let app = axum::Router::new()
            .route("/environment", axum::routing::patch(|| async {}))
            .layer(cors_layer());
        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("/environment")
            .header(header::ORIGIN, "https://example.com")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PATCH")
            .header(
                header::ACCESS_CONTROL_REQUEST_HEADERS,
                "authorization, content-type, spacetime-environment, spacetime-environment-remove",
            )
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let allowed = response.headers()[header::ACCESS_CONTROL_ALLOW_HEADERS]
            .to_str()
            .unwrap();
        for name in ["spacetime-environment", "spacetime-environment-remove"] {
            assert!(allowed.split(',').any(|h| h.trim() == name), "{name} not in {allowed}");
        }
    }
}
