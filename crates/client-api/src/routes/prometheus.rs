use axum::extract::{FromRef, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::{log_and_500, ControlCtx, ControlNodeDelegate};

#[derive(Serialize, Deserialize)]
struct SDConfig {
    targets: Vec<String>,
    labels: HashMap<String, String>,
}

pub async fn get_sd_config(State(ctx): State<Arc<dyn ControlCtx>>) -> axum::response::Result<impl IntoResponse> {
    // TODO(cloutiertyler): security
    let nodes = ctx.get_nodes().await.map_err(log_and_500)?;

    let mut targets = Vec::new();
    let labels = HashMap::new();

    for node in nodes {
        targets.push(node.advertise_addr);
    }

    let sd_config = SDConfig { targets, labels };

    Ok(axum::Json(vec![sd_config]))
}

pub fn router<S>() -> axum::Router<S>
where
    S: ControlNodeDelegate + Clone + 'static,
    Arc<dyn ControlCtx>: FromRef<S>,
{
    use axum::routing::get;
    axum::Router::new().route("/sd_config", get(get_sd_config))
}
