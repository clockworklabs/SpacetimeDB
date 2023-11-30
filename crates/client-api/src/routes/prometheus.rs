use std::collections::HashMap;

use axum::extract::State;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::{log_and_500, ControlStateReadAccess};

#[derive(Serialize, Deserialize)]
struct SDConfig {
    targets: Vec<String>,
    labels: HashMap<String, String>,
}

pub async fn get_sd_config<S: ControlStateReadAccess>(
    State(ctx): State<S>,
) -> axum::response::Result<impl IntoResponse> {
    // TODO(cloutiertyler): security
    let nodes = ctx.get_nodes().map_err(log_and_500)?;

    let mut targets = Vec::new();
    let labels = HashMap::new();

    for node in nodes {
        if let Some(addr) = node.advertise_addr {
            targets.push(addr);
        }
    }

    let sd_config = SDConfig { targets, labels };

    Ok(axum::Json(vec![sd_config]))
}

pub fn router<S>() -> axum::Router<S>
where
    S: ControlStateReadAccess + Clone + Send + Sync + 'static,
{
    use axum::routing::get;
    axum::Router::new().route("/sd_config", get(get_sd_config::<S>))
}
