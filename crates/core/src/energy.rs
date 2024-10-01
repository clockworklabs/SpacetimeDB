use std::time::Duration;

use spacetimedb_lib::{Hash, Identity};

use crate::messages::control_db::Database;

pub use spacetimedb_client_api_messages::energy::*;
pub struct ReducerFingerprint<'a> {
    pub module_hash: Hash,
    pub module_identity: Identity,
    pub caller_identity: Identity,
    pub reducer_name: &'a str,
}

pub trait EnergyMonitor: Send + Sync + 'static {
    fn reducer_budget(&self, fingerprint: &ReducerFingerprint<'_>) -> ReducerBudget;
    fn record_reducer(
        &self,
        fingerprint: &ReducerFingerprint<'_>,
        energy_used: EnergyQuanta,
        execution_duration: Duration,
    );
    fn record_disk_usage(&self, database: &Database, replica_id: u64, disk_usage: u64, period: Duration);
    fn record_memory_usage(&self, database: &Database, replica_id: u64, mem_usage: u64, period: Duration);
    fn record_query_energy(&self, owner_identity: Identity, replica_id: u64, cpu_usage: Duration);
}

#[derive(Default)]
pub struct NullEnergyMonitor;

impl EnergyMonitor for NullEnergyMonitor {
    fn reducer_budget(&self, _fingerprint: &ReducerFingerprint<'_>) -> ReducerBudget {
        ReducerBudget::DEFAULT_BUDGET
    }

    fn record_reducer(
        &self,
        _fingerprint: &ReducerFingerprint<'_>,
        _energy_used: EnergyQuanta,
        _execution_duration: Duration,
    ) {
    }

    fn record_disk_usage(&self, _database: &Database, _replica_id: u64, _disk_usage: u64, _period: Duration) {}

    fn record_memory_usage(&self, _database: &Database, _replica_id: u64, _mem_usage: u64, _period: Duration) {}

    fn record_query_energy(&self, _owner_identity: Identity, _replica_id: u64, _cpu_usage: Duration) {}
}
