use crate::database_logger::DatabaseLogger;
use crate::db::relational_db::tests_utils::TestDB;
use crate::energy::NullEnergyMonitor;
use crate::host::Scheduler;
use crate::messages::control_db::{Database, HostType};
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use spacetimedb_lib::{Hash, Identity};
use std::sync::Arc;

pub(crate) fn module_creation_context_for_test(
    host_type: HostType,
) -> (ModuleCreationContext, tokio::runtime::Runtime) {
    let TestDB { db, .. } = TestDB::in_memory().expect("failed to make test db");
    let (subscriptions, runtime) = ModuleSubscriptions::for_test_new_runtime(db.clone());
    let logger = {
        let _rt = runtime.enter();
        Arc::new(DatabaseLogger::in_memory(64 * 1024))
    };
    let replica_ctx = Arc::new(ReplicaContext {
        database: Database {
            id: 0,
            database_identity: Identity::ZERO,
            owner_identity: Identity::ZERO,
            host_type,
            initial_program: Hash::ZERO,
        },
        replica_id: 0,
        logger,
        subscriptions,
        relational_db: db,
    });
    let (scheduler, _starter) = Scheduler::open(replica_ctx.relational_db.clone());

    (
        ModuleCreationContext {
            replica_ctx,
            scheduler,
            program_hash: Hash::ZERO,
            energy_monitor: Arc::new(NullEnergyMonitor),
        },
        runtime,
    )
}
