use gotham::prelude::{DefineSingleRoute, DrawRoutes};
use gotham::router::route::matcher::AnyRouteMatcher;
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::db::db_metrics;
use std::sync::Arc;

use super::worker_db::{WorkerDb, WORKER_DB};
use super::worker_metrics;

mod proxy;

struct WorkerEnv {
    worker_db: &'static WorkerDb,
    db_inst_ctx_controller: Arc<DatabaseInstanceContextController>,
}

trait IntoResult<T> {
    fn into_result(self) -> T;
}
impl<T> IntoResult<T> for T {
    fn into_result(self) -> T {
        self
    }
}
impl<T, E> IntoResult<Result<T, E>> for T {
    fn into_result(self) -> Result<T, E> {
        Ok(self)
    }
}

spacetimedb_client_api::delegate_databasedb!(for WorkerEnv, self to self.worker_db, |x| x.into_result());

impl spacetimedb_client_api::ApiCtx for WorkerEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        let mut metric_families = worker_metrics::REGISTRY.gather();
        metric_families.extend(db_metrics::REGISTRY.gather());
        metric_families
    }

    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController {
        &self.db_inst_ctx_controller
    }
}

pub async fn start(db_inst_ctx_controller: Arc<DatabaseInstanceContextController>, listen_addr: String) -> ! {
    let worker_db = &*WORKER_DB;
    let ctx = WorkerEnv {
        worker_db,
        db_inst_ctx_controller,
    };
    spacetimedb_client_api::start_customized(Arc::new(ctx), listen_addr, |route| {
        let proxied_routes = [
            "/database/dns/:database_name",
            "/database/reverse_dns/:address",
            "/database/register_tld",
            "/database/set_name",
            "/database/publish",
            "/database/delete",
            "/identity/",
            "/identity/*",
            "/node/",
            "/node/*",
            "/energy/",
            "/energy/*",
        ];
        for path in proxied_routes {
            route
                .request(AnyRouteMatcher::new(), path)
                .to_async(proxy::proxy_to_control_node_client_api);
        }
    })
    .await
}
