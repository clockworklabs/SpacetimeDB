use axum::routing::MethodRouter;
use http::header;
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

    let cors = cors::CorsLayer::new()
        .allow_headers([header::AUTHORIZATION, header::ACCEPT, header::CONTENT_TYPE])
        .allow_methods(cors::Any)
        .allow_origin(cors::Any);

    axum::Router::new()
        .nest("/v1", router.layer(cors))
        .nest("/internal", internal::router())
}
