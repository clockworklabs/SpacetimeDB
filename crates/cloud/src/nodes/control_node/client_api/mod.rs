use std::sync::Arc;

use spacetimedb::control_db::{ControlDb, CONTROL_DB};
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::protobuf::control_db::Database;
use spacetimedb::worker_database_instance::WorkerDatabaseInstance;

use super::controller::Controller;

struct ControlEnv {
    control_db: &'static ControlDb,
    controller: Controller,
    db_inst_ctx_controller: Arc<DatabaseInstanceContextController>,
}

spacetimedb_client_api::delegate_databasedb!(for ControlEnv, self to self.control_db, |x| x.await);

#[async_trait::async_trait]
impl spacetimedb_client_api::ApiCtx for ControlEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        super::prometheus_metrics::REGISTRY.gather()
    }

    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController {
        &self.db_inst_ctx_controller
    }

    async fn load_database_instance(
        &self,
        _db: Database,
        _instance_id: u64,
    ) -> anyhow::Result<(Arc<WorkerDatabaseInstance>, spacetimedb::util::IVec)> {
        unimplemented!()
    }
}

spacetimedb_client_api::delegate_controller!(for ControlEnv, self to self.controller);

pub async fn start(
    db_inst_ctx_controller: Arc<DatabaseInstanceContextController>,
    controller: Controller,
    addr: String,
) -> ! {
    let control_db = &*CONTROL_DB;
    let ctx = ControlEnv {
        control_db,
        controller,
        db_inst_ctx_controller,
    };
    spacetimedb_client_api::start_control(Arc::new(ctx), addr, |_| {}).await
}
