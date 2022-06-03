use crate::api;
use gotham::anyhow::anyhow;
use gotham::handler::HandlerError;
use gotham::handler::SimpleHandlerResult;
use gotham::prelude::FromState;
use gotham::prelude::StaticResponseExtender;
use gotham::router::builder::*;
use gotham::router::Router;
use gotham::state::State;
use gotham::state::StateData;
use hyper::body::HttpBody;
use hyper::Body;
use hyper::{Response, StatusCode};
use serde::Deserialize;

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct InitModuleParams {
    identity: String,
    name: String,
}

async fn init_module(state: &mut State) -> SimpleHandlerResult {
    let InitModuleParams { identity, name } = InitModuleParams::take_from(state);
    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => {
            return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST))
        }
    };
    let wasm_bytes = data.to_vec();

    match api::database::init_module(&identity, &name, wasm_bytes).await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e);
            return Err(HandlerError::from(e));
        }
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();

    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct CallParams {
    identity: String,
    name: String,
    reducer: String,
}

async fn call(state: &mut State) -> SimpleHandlerResult {
    let CallParams {
        identity,
        name,
        reducer,
    } = CallParams::take_from(state);
    let body = state.borrow_mut::<Body>();
    let data = body.data().await;
    if data.is_none() {
        return Err(HandlerError::from(anyhow!("Missing request body.")).with_status(StatusCode::BAD_REQUEST));
    }
    let data = data.unwrap();
    let arg_bytes = data.unwrap().to_vec();

    match api::database::call(&identity, &name, reducer, arg_bytes).await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e)
        }
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();

    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct LogsParams {
    identity: String,
    name: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct LogsQuery {
    num_lines: u32,
}

async fn logs(state: &mut State) -> SimpleHandlerResult {
    let LogsParams { identity, name } = LogsParams::take_from(state);
    let LogsQuery { num_lines } = LogsQuery::take_from(state);

    let lines = api::database::logs(&identity, &name, num_lines).await;

    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(lines))
        .unwrap();

    Ok(res)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route.get("/").to(|state| (state, "Hello, World!"));
        route
            .post("/database/init/:identity/:name")
            .with_path_extractor::<InitModuleParams>()
            .to_async_borrowing(init_module);
        route
            .post("/database/call/:identity/:name/:reducer")
            .with_path_extractor::<CallParams>()
            .to_async_borrowing(call);
        route
            .get("/database/logs/:identity/:name")
            .with_path_extractor::<LogsParams>()
            .with_query_string_extractor::<LogsQuery>()
            .to_async_borrowing(logs);
        // route.delegate("/auth").to_router(auth_router());
        // route.delegate("/admin").to_router(admin_router());
        // route.delegate("/metrics").to_router(metrics_router());
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gotham::test::TestServer;

    #[test]
    fn init_database() {
        let test_server = TestServer::new(router()).unwrap();
        let uri = "http://localhost/database/init/clockworklabs/bitcraft";
        let body = Body::empty();
        let mime = "application/octet-stream".parse().unwrap();
        let response = test_server.client().post(uri, body, mime).perform().unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
