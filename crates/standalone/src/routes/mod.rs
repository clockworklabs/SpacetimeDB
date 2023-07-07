use http::header::{ACCEPT, AUTHORIZATION};
use tower_http::cors::{Any, CorsLayer};

use spacetimedb_client_api::{
    routes::{database, energy, identity, metrics, prometheus},
    ControlStateDelegate, ModuleHostContextLoader, NodeDelegate,
};

#[allow(clippy::let_and_return)]
pub fn router<S>() -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + ModuleHostContextLoader + Clone + 'static,
{
    let router = axum::Router::new()
        .nest("/database", database::control_routes().merge(database::worker_routes()))
        .nest("/identity", identity::router())
        .nest("/energy", energy::router())
        .nest("/prometheus", prometheus::router())
        .nest("/metrics", metrics::router());

    #[cfg(feature = "tracelogging")]
    let router = router.nest("/tracelog", spacetimedb_client_api::routes::tracelog::router());

    let cors = CorsLayer::new()
        .allow_headers([AUTHORIZATION, ACCEPT])
        .allow_methods(Any)
        .allow_origin(Any);

    router.layer(cors)
}
