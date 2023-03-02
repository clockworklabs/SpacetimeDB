pub mod controller;
mod worker_db;

use controller::Controller;
use spacetimedb::control_db::CONTROL_DB;
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::db::db_metrics;
use spacetimedb::object_db::ObjectDb;
use spacetimedb::worker_metrics;
use worker_db::WorkerDb;

pub struct StandaloneEnv {
    controller: Controller,
}

impl StandaloneEnv {
    pub fn init() -> anyhow::Result<Self> {
        let worker_db = WorkerDb::init()?;
        let object_db = ObjectDb::init()?;
        let db_inst_ctx_controller = DatabaseInstanceContextController::new();
        let control_db = &*CONTROL_DB;
        Ok(Self {
            controller: Controller::new(worker_db, control_db, db_inst_ctx_controller, object_db),
        })
    }
}

spacetimedb_client_api::delegate_databasedb!(for StandaloneEnv, self to self.controller, |x| x.await);
spacetimedb_client_api::delegate_controller!(for StandaloneEnv, self to self.controller);

impl spacetimedb_client_api::ApiCtx for StandaloneEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        let mut metric_families = worker_metrics::REGISTRY.gather();
        metric_families.extend(db_metrics::REGISTRY.gather());
        metric_families
    }

    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController {
        &self.controller.db_inst_ctx_controller
    }
}
