use spacetimedb_lib::Hash;

use crate::database_instance_context::DatabaseInstanceContext;
use crate::db::relational_db::ConnectedClients;
use crate::energy::EnergyMonitor;
use crate::host::scheduler::{Scheduler, SchedulerStarter};
use crate::messages::control_db::HostType;
use crate::util::AnyBytes;
use std::sync::Arc;

pub struct ModuleHostContext {
    pub dbic: Arc<DatabaseInstanceContext>,
    pub scheduler: Scheduler,
    pub scheduler_starter: SchedulerStarter,
    pub host_type: HostType,
    pub program_bytes: AnyBytes,
    pub dangling_client_connections: Option<ConnectedClients>,
}

pub struct ModuleCreationContext {
    pub dbic: Arc<DatabaseInstanceContext>,
    pub scheduler: Scheduler,
    pub program_bytes: AnyBytes,
    pub program_hash: Hash,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
}
