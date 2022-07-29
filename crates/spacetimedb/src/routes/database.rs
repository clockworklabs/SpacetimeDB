use crate::api;
use crate::auth::get_creds_from_header;
use crate::auth::invalid_token_res;
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
use hyper::header::AUTHORIZATION;
use hyper::Body;
use hyper::HeaderMap;
use hyper::{Response, StatusCode};
use serde::Deserialize;

use super::subscribe::handle_websocket;
use super::subscribe::SubscribeParams;

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct InitModuleParams {
    identity: String,
    name: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct InitModuleQueryParams {
    force: Option<bool>,
}

async fn init_module(state: &mut State) -> SimpleHandlerResult {
    let InitModuleParams { identity, name } = InitModuleParams::take_from(state);
    let InitModuleQueryParams { force } = InitModuleQueryParams::take_from(state);
    let force = force.unwrap_or(false);
    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST)),
    };
    let wasm_bytes = data.to_vec();

    match api::database::init_module(&identity, &name, force, wasm_bytes).await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e.backtrace());
            return Err(HandlerError::from(e));
        }
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();

    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DeleteModuleParams {
    identity: String,
    name: String,
}

async fn delete_module(state: &mut State) -> SimpleHandlerResult {
    let DeleteModuleParams { identity, name } = DeleteModuleParams::take_from(state);
    match api::database::delete_module(&identity, &name).await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e.backtrace());
            return Err(HandlerError::from(e));
        }
    }
    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct UpdateModuleParams {
    identity: String,
    name: String,
}

async fn update_module(state: &mut State) -> SimpleHandlerResult {
    let UpdateModuleParams { identity, name } = UpdateModuleParams::take_from(state);
    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST)),
    };
    let wasm_bytes = data.to_vec();

    match api::database::update_module(&identity, &name, wasm_bytes).await {
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
    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);
    let (caller_identity, caller_identity_token) = if let Some(auth_header) = auth_header {
        // Validate the credentials of this connection
        match get_creds_from_header(auth_header) {
            Ok(v) => v,
            Err(_) => return Ok(invalid_token_res()),
        }
    } else {
        // Generate a new identity if this request doesn't have one already
        let (identity, identity_token) = api::spacetime_identity().await.unwrap();
        (identity, identity_token)
    };

    let body = state.borrow_mut::<Body>();
    let data = body.data().await;
    if data.is_none() {
        return Err(HandlerError::from(anyhow!("Missing request body.")).with_status(StatusCode::BAD_REQUEST));
    }
    let data = data.unwrap();
    let arg_bytes = data.unwrap().to_vec();

    match api::database::call(&identity, &name, caller_identity, reducer, arg_bytes).await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e)
        }
    }

    let res = Response::builder()
        .header("Spacetime-Identity", hex::encode(caller_identity))
        .header("Spacetime-Identity-Token", caller_identity_token)
        .status(StatusCode::OK)
        .body(Body::empty())
        .unwrap();
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
        route
            .get("/:identity/:name/subscribe")
            .with_path_extractor::<SubscribeParams>()
            .to_async(handle_websocket);

        route
            .post("/:identity/:name/init")
            .with_path_extractor::<InitModuleParams>()
            .with_query_string_extractor::<InitModuleQueryParams>()
            .to_async_borrowing(init_module);

        route
            .post("/:identity/:name/delete")
            .with_path_extractor::<DeleteModuleParams>()
            .to_async_borrowing(delete_module);

        route
            .post("/:identity/:name/update")
            .with_path_extractor::<UpdateModuleParams>()
            .to_async_borrowing(update_module);

        route
            .post("/:identity/:name/call/:reducer")
            .with_path_extractor::<CallParams>()
            .to_async_borrowing(call);

        route
            .get("/:identity/:name/logs")
            .with_path_extractor::<LogsParams>()
            .with_query_string_extractor::<LogsQuery>()
            .to_async_borrowing(logs);
    })
}
