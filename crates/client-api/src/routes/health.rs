use crate::{ControlStateDelegate, NodeDelegate};
use axum::extract::State;
use axum::response::IntoResponse;
use http::StatusCode;

static VERSION: &str = selected_version(option_env!("SPACETIMEDB_VERSION"), env!("CARGO_PKG_VERSION"));
static PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");

const fn selected_version(release_version: Option<&'static str>, cargo_version: &'static str) -> &'static str {
    match release_version {
        Some(version) => version,
        None => cargo_version,
    }
}

pub async fn health<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
) -> axum::response::Result<impl IntoResponse> {
    let nodes: Vec<u64> = ctx
        .get_nodes()
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Couldn't connect to the control database",
            )
        })?
        .iter()
        .map(|n| n.id)
        .collect();
    let schedulable = !ctx
        .get_node_by_id(
            ctx.get_node_id()
                .await
                .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "Can't get node id"))?,
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get node info"))?
        .map(|n| n.unschedulable)
        .unwrap_or(false);

    Ok(axum::Json(serde_json::json!({
        "package_name": PACKAGE_NAME,
        "version": VERSION,
        "nodes": nodes,
        "schedulable": schedulable,
    })))
}

pub fn router<S>() -> axum::Router<S>
where
    S: ControlStateDelegate + NodeDelegate + Clone + 'static,
{
    use axum::routing::get;
    axum::Router::new().route("/", get(health::<S>))
}

#[cfg(test)]
mod tests {
    use super::selected_version;

    #[test]
    fn selected_version_uses_release_version_when_present() {
        assert_eq!(selected_version(Some("2.7.0-hotfix3"), "2.7.0"), "2.7.0-hotfix3");
    }

    #[test]
    fn selected_version_falls_back_to_cargo_version() {
        assert_eq!(selected_version(None, "2.8.0"), "2.8.0");
    }
}
