use std::sync::Arc;

use spacetimedb::control_db::{ControlDb, CONTROL_DB};

use super::controller::Controller;

struct ControlEnv {
    control_db: &'static ControlDb,
    controller: Controller,
}

spacetimedb_client_api::delegate_databasedb!(for ControlEnv, self to self.control_db, |x| x.await);

impl spacetimedb_client_api::ApiCtx for ControlEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        super::prometheus_metrics::REGISTRY.gather()
    }
}

spacetimedb_client_api::delegate_controller!(for ControlEnv, self to self.controller);

pub async fn start(addr: String) -> ! {
    let control_db = &*CONTROL_DB;
    let ctx = ControlEnv {
        control_db,
        controller: Controller::new(control_db),
    };
    spacetimedb_client_api::start_control(Arc::new(ctx), addr, |_| {}).await
}
