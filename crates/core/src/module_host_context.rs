use crate::energy::EnergyMonitor;
use crate::host::scheduler::Scheduler;
use crate::replica_context::ReplicaContext;
use spacetimedb_datastore::traits::Program;
use spacetimedb_sats::hash::Hash;
use std::sync::Arc;

pub struct ModuleCreationContext<'a> {
    pub replica_ctx: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub program: &'a Program,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
}

impl ModuleCreationContext<'_> {
    pub fn into_limited(self) -> ModuleCreationContextLimited {
        ModuleCreationContextLimited {
            replica_ctx: self.replica_ctx,
            scheduler: self.scheduler,
            program_hash: self.program.hash,
            energy_monitor: self.energy_monitor,
        }
    }
}

pub struct ModuleCreationContextLimited {
    pub replica_ctx: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub program_hash: Hash,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
}
