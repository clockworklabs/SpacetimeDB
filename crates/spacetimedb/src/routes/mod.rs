mod subscribe;
mod database;
mod identity;
use gotham::{router::{build_simple_router, Router}, prelude::*};
use database::router as database_router;
use identity::router as identity_router;

pub fn router() -> Router {
    build_simple_router(|route| {
        // route.delegate("/metrics").to_router(metrics_router());
        route.delegate("/database/:identity/:name").to_router(database_router());
        route.delegate("/identity").to_router(identity_router());
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gotham::test::TestServer;
    use hyper::{Body, StatusCode};

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
