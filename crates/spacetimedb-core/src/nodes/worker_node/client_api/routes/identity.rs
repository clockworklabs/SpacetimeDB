use crate::nodes::worker_node::client_api::proxy::proxy_to_control_node_client_api;
use gotham::router::route::matcher::AnyRouteMatcher;
use gotham::{
    prelude::*,
    router::{build_simple_router, Router},
};

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .request(AnyRouteMatcher::new(), "/")
            .to_async(proxy_to_control_node_client_api);
        route
            .request(AnyRouteMatcher::new(), "/*")
            .to_async(proxy_to_control_node_client_api);
    })
}
