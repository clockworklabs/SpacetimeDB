use spacetimedb::control_db::ControlDb;
use spacetimedb::energy::{EnergyBalance, EnergyMonitor, EnergyQuanta, ReducerBudget, ReducerFingerprint};
use spacetimedb::messages::control_db::Database;
use spacetimedb_lib::Identity;
use std::time::Duration;

pub(crate) struct StandaloneEnergyMonitor {
    control_db: ControlDb,
}

impl StandaloneEnergyMonitor {
    pub fn new(control_db: ControlDb) -> Self {
        Self { control_db }
    }

    fn withdraw_energy(&self, identity: Identity, amount: EnergyQuanta) {
        if amount.get() == 0 {
            return;
        }
        crate::withdraw_energy(&self.control_db, &identity, amount).unwrap();
    }
}

impl EnergyMonitor for StandaloneEnergyMonitor {
    fn reducer_budget(&self, _fingerprint: &ReducerFingerprint<'_>) -> ReducerBudget {
        // Infinitely large reducer budget in Standalone
        ReducerBudget::new(u64::MAX)
    }

    fn record_reducer(
        &self,
        fingerprint: &ReducerFingerprint<'_>,
        energy_used: EnergyQuanta,
        _execution_duration: Duration,
    ) {
        self.withdraw_energy(fingerprint.module_identity, energy_used)
    }

    fn record_disk_usage(&self, database: &Database, _instance_id: u64, disk_usage: u64, period: Duration) {
        let amount = EnergyQuanta::from_disk_usage(disk_usage, period);
        self.withdraw_energy(database.identity, amount)
    }
}

impl StandaloneEnergyMonitor {
    /// To be used if we ever want to enable reducer budgets in Standalone
    fn _reducer_budget(&self, fingerprint: &ReducerFingerprint<'_>) -> ReducerBudget {
        let balance = self
            .control_db
            .get_energy_balance(&fingerprint.module_identity)
            .unwrap()
            .unwrap_or(EnergyBalance::ZERO);
        // clamp it
        let balance = balance.to_energy_quanta().unwrap_or(EnergyQuanta::ZERO);
        ReducerBudget::from_energy(balance).unwrap_or(ReducerBudget::MAX)
    }
}
