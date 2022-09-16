mod database;
mod identity;
mod metrics;
mod node;
pub mod subscribe;
use database::router as database_router;
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
        route.delegate("/identity").to_router(identity_router());
        route.delegate("/node").to_router(node_router());
        route.delegate("/metrics").to_router(metrics_router());
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gotham::test::TestServer;
    use hyper::{Body, StatusCode};

    #[ignore] // Disabled for now.
    #[test]
    fn init_database() {
        let test_server = TestServer::new(router()).unwrap();
        let uri = "http://localhost/database/init/clockworklabs/bitcraft";
        let body = Body::empty();
        let mime = "application/octet-stream".parse().unwrap();
        let response = test_server.client().post(uri, body, mime).perform().unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
