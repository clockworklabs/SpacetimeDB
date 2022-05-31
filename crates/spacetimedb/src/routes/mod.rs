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

    // // println!("{}", String::from_utf8(wat.to_owned()).unwrap());

    // let wasm_bytes = wat2wasm(&wat)?.to_vec();
    // let hex_identity = hex::encode(hash_bytes(""));
    // let name = "test";
    // if let Err(e) = api::database::init_module(&hex_identity, name, wasm_bytes).await {
    //     // TODO: check if it failed because it's already been created
    //     log::error!("{:?}", e);
    // }

    // let reducer: String = "test".into();

    // // TODO: actually handle args
    // let arg_str = r#"[{"x": 0, "y": 1, "z": 2}, {"foo": "This is a string."}]"#;
    // let arg_bytes = arg_str.as_bytes().to_vec();
    // api::database::call(&hex_identity, &name, reducer.clone(), arg_bytes.clone()).await?;
    // api::database::call(&hex_identity, &name, reducer, arg_bytes).await?;

    // println!("logs:");
    // println!("{}", api::database::logs(&hex_identity, &name, 10).await);

    // let (identity, token) = api::spacetime_identity().await?;
    // println!("identity: {:?}", identity);
    // println!("token: {}", token);

    // api::spacetime_identity_associate_email("tyler@clockworklabs.io", &token).await?;
    // //////////////////

async fn init_module(state: &mut State) -> SimpleHandlerResult {
    let InitModuleParams { identity, name } = InitModuleParams::take_from(state);
    let body = state.borrow_mut::<Body>();
    let data = body.data().await;
    if data.is_none() {
        return Err(HandlerError::from(anyhow!("Missing request body.")).with_status(StatusCode::BAD_REQUEST));
    }
    let data = data.unwrap();
    let wasm_bytes = data.unwrap().to_vec();

    match api::database::init_module(&identity, &name, wasm_bytes).await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e)
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
    let CallParams { identity, name, reducer } = CallParams::take_from(state);
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

    let res = Response::builder().status(StatusCode::OK).body(Body::from(lines)).unwrap();

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
