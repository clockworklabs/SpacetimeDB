#[macro_export]
macro_rules! with_ctx {
    ($ctx:expr, $f:expr) => {{
        let x = std::panic::AssertUnwindSafe(($ctx.clone(), $f));
        move || {
            let (ctx, f) = x.clone();
            Ok(move |mut state| -> std::pin::Pin<Box<gotham::handler::HandlerFuture>> {
                Box::pin(async move {
                    match f(std::borrow::Borrow::borrow(&ctx), &mut state).await {
                        Ok(x) => Ok((state, x)),
                        Err(e) => Err((state, e)),
                    }
                })
            })
        }
    }};
}

mod database;
mod energy;
mod identity;
mod metrics;
mod prometheus;
pub mod subscribe;
mod util;

#[cfg(feature = "tracelogging")]
mod tracelog;

use std::sync::Arc;

use gotham::{
    prelude::*,
    router::{build_simple_router, Router},
};

use crate::{ApiCtx, ControllerCtx};

pub fn router(
    ctx: Arc<dyn ApiCtx>,
    control_ctx: Option<Arc<dyn ControllerCtx>>,
    customize: impl FnOnce(&mut gotham::router::builder::RouterBuilder<'_, (), ()>),
) -> Router {
    build_simple_router(|route| {
        route
            .delegate("/database")
            .to_router(database::router(&ctx, control_ctx.as_ref()));
        route.delegate("/identity").to_router(identity::router());
        route.delegate("/energy").to_router(energy::router());
        route.delegate("/prometheus").to_router(prometheus::router());
        route.delegate("/metrics").to_router(metrics::router(&ctx));
        #[cfg(feature = "tracelogging")]
        route.delegate("/tracelog").to_router(tracelog::router());
        customize(route);
    })
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use gotham::test::TestServer;
//     use hyper::{Body, StatusCode};

//     #[ignore] // Disabled for now.
//     #[test]
//     fn init_database() {
//         let test_server = TestServer::new(router()).unwrap();
//         let uri = "http://localhost/database/publish/clockworklabs/bitcraft";
//         let body = Body::empty();
//         let mime = "application/octet-stream".parse().unwrap();
//         let response = test_server.client().post(uri, body, mime).perform().unwrap();

//         assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
//     }
// }
