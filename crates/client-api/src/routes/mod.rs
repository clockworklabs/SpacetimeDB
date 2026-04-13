use http::header;
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
        .nest("/v1", router.layer(cors))
        .nest("/internal", internal::router())
}
