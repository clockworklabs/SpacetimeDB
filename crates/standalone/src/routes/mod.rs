use http::header::{ACCEPT, AUTHORIZATION};
use tower_http::cors::{Any, CorsLayer};

use spacetimedb_client_api::{
    routes::{database, energy, identity, metrics, prometheus},
    ControlStateDelegate, NodeDelegate,
};

#[allow(clippy::let_and_return)]
pub fn router<S>(ctx: S) -> axum::Router<()>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    let router = axum::Router::new()
        .nest(
            "/database",
            database::control_routes(ctx.clone()).merge(database::worker_routes(ctx.clone())),
        )
        .nest("/identity", identity::router(ctx.clone()))
        .nest("/energy", energy::router())
        .nest("/prometheus", prometheus::router())
        .nest("/metrics", metrics::router());

    let cors = CorsLayer::new()
        .allow_headers([AUTHORIZATION, ACCEPT])
        .allow_methods(Any)
        .allow_origin(Any);

    router.layer(cors).with_state(ctx)
}
