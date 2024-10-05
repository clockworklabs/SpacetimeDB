use crate::replica_context::ReplicaContext;
use crate::db::datastore::traits::Program;
use crate::energy::EnergyMonitor;
use crate::host::scheduler::Scheduler;
use std::sync::Arc;

pub struct ModuleCreationContext<'a> {
    pub dbic: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub program: &'a Program,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
}
