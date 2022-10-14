mod database;
mod energy;
mod identity;
mod metrics;
mod node;
mod prometheus;

use self::prometheus::router as prometheus_router;
use database::router as database_router;
use energy::router as energy_router;
use gotham::{
    prelude::*,
    router::{build_simple_router, Router},
};
use identity::router as identity_router;
use metrics::router as metrics_router;
use node::router as node_router;

pub fn router() -> Router {
    build_simple_router(|route| {
        route.delegate("/database").to_router(database_router());
        route.delegate("/energy").to_router(energy_router());
        route.delegate("/identity").to_router(identity_router());
        route.delegate("/node").to_router(node_router());
        route.delegate("/prometheus").to_router(prometheus_router());
        route.delegate("/metrics").to_router(metrics_router());
    })
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use gotham::test::TestServer;
//     use hyper
