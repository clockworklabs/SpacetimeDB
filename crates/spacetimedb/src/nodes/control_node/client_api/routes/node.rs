use crate::{json::control_db::NodeJson, nodes::control_node::control_db};
use gotham::{
    handler::SimpleHandlerResult,
    prelude::*,
    router::{build_simple_router, Router},
    state::State,
};
use hyper::{Body, Response, StatusCode};

async fn get_nodes(_state: &mut State) -> SimpleHandlerResult {
    // TODO(cloutiertyler): security
    let nodes = control_db::get_nodes().await?;

    let mut json_nodes = Vec::new();
    for node in nodes {
        json_nodes.push(NodeJson {
            id: node.id,
            unschedulable: node.unschedulable,
            advertise_addr: node.advertise_addr,
        });
    }

    let json = serde_json::to_string(&json_nodes).unwrap();
    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap();

    Ok(res)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route.get("/").to_async_borrowing(get_nodes);
    })
}
