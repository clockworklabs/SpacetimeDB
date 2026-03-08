use crate::energy::EnergyMonitor;
use crate::host::scheduler::Scheduler;
use crate::replica_context::ReplicaContext;
#[cfg(feature = "onnx")]
use spacetimedb_paths::server::ServerDataDir;
use spacetimedb_sats::hash::Hash;
use std::sync::Arc;

pub struct ModuleCreationContext {
    pub replica_ctx: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub program_hash: Hash,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
    #[cfg(feature = "onnx")]
    pub data_dir: Option<Arc<ServerDataDir>>,
}
