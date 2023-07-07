use std::collections::HashMap;

use axum::extract::State;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::{log_and_500, ControlStateDelegate};

#[derive(Serialize, Deserialize)]
struct SDConfig {
    targets: Vec<String>,
    labels: HashMap<String, String>,
}

pub async fn get_sd_config<S: ControlStateDelegate>(State(ctx): State<S>) -> axum::response::Result<impl IntoResponse> {
    // TODO(cloutiertyler): security
    let nodes = ctx.get_nodes().map_err(log_and_500)?;

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
    S: ControlStateDelegate + Clone + 'static,
{
    use axum::routing::get;
    axum::Router::new().route("/sd_config", get(get_sd_config::<S>))
}
