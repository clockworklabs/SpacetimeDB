use crate::energy::EnergyMonitor;
use crate::host::scheduler::Scheduler;
use crate::replica_context::ReplicaContext;
use spacetimedb_sats::hash::Hash;
use std::sync::Arc;

pub struct ModuleCreationContext {
    pub replica_ctx: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub program_hash: Hash,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
}
