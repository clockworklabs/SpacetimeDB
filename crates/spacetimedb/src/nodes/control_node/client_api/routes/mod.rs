mod database;
mod identity;
mod node;
use database::router as database_router;
use gotham::{
    prelude::*,
    router::{build_simple_router, Router},
};
use identity::router as identity_router;
use node::router as node_router;

pub fn router() -> Router {
    build_simple_router(|route| {
        route.delegate("/node").to_router(node_router());
        route.delegate("/database").to_router(database_router());
        route.delegate("/identity").to_router(identity_router());
    })
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use gotham::test::TestServer;
//     use hyper