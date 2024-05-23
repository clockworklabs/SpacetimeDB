use spacetimedb_lib::Hash;

use crate::database_instance_context::DatabaseInstanceContext;
use crate::energy::EnergyMonitor;
use crate::host::scheduler::Scheduler;
use crate::util::AnyBytes;
use std::sync::Arc;

pub struct ModuleCreationContext {
    pub dbic: Arc<DatabaseInstanceContext>,
    pub scheduler: Scheduler,
    pub program_bytes: AnyBytes,
    pub program_hash: Hash,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
}
