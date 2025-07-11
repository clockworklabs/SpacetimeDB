use crate::energy::EnergyMonitor;
use crate::host::scheduler::Scheduler;
use crate::replica_context::ReplicaContext;
use spacetimedb_datastore::traits::Program;
use std::sync::Arc;

pub struct ModuleCreationContext<'a> {
    pub replica_ctx: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub program: &'a Program,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
}
