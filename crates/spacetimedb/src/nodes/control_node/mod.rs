pub(crate) mod worker_api;
pub(crate) mod client_api; // TODO: should be private
pub(crate) mod control_db;
mod object_db;
mod controller;

use futures::{future::join_all, FutureExt};
use crate::postgres;

pub async fn start(config: crate::nodes::node_config::NodeConfig) {
    postgres::init().await;
    join_all(vec![
        worker_api::start(config).boxed(),
        client_api::start(26258).boxed(),
    ]).await;
}