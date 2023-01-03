use gotham::{
    handler::SimpleHandlerResult,
    prelude::*,
    router::{build_simple_router, Router},
    state::State,
};
use hyper::{Body, Response, StatusCode};
use serde::{Deserialize, Serialize};
use spacetimedb::control_db;
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
struct SDConfig {
    targets: Vec<String>,
    labels: HashMap<String, String>,
}

async fn get_sd_config(_state: &mut State) -> SimpleHandlerResult {
    // TODO(cloutiertyler): security
    let nodes = control_db::get_nodes().await?;

    let mut targets = Vec::new();
    let labels = HashMap::new();

    for node in nodes {
        targets.push(node.advertise_addr);
    }

    let sd_config = SDConfig { targets, labels };

    let json = serde_json::to_string(&vec![sd_config]).unwrap();
    let res = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(json))
        .unwrap();

    Ok(res)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route.get("/sd_config").to_async_borrowing(get_sd_config);
    })
}
