use crate::StandaloneEnv;
use spacetimedb::host::{EnergyDiff, EnergyMonitor, EnergyMonitorFingerprint, EnergyQuanta};
use spacetimedb_client_api::ControlStateWriteAccess;
use std::{
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

pub(crate) struct StandaloneEnergyMonitor {
    inner: Arc<Mutex<Inner>>,
}

impl StandaloneEnergyMonitor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                standalone_env: Weak::new(),
            })),
        }
    }

    pub fn set_standalone_env(&self, standalone_env: Arc<StandaloneEnv>) {
        self.inner.lock().unwrap().set_standalone_env(standalone_env);
    }
}

impl EnergyMonitor for StandaloneEnergyMonitor {
    fn reducer_budget(&self, _fingerprint: &EnergyMonitorFingerprint<'_>) -> EnergyQuanta {
        // Infinitely large reducer budget in Standalone
        EnergyQuanta::new(i128::max_value())
    }

    fn record(
        &self,
        fingerprint: &EnergyMonitorFingerprint<'_>,
        energy_used: EnergyDiff,
        _execution_duration: Duration,
    ) {
        if energy_used.0 == 0 {
            return;
        }
        let module_identity = fingerprint.module_identity;
        let standalone_env = {
            self.inner
                .lock()
                .unwrap()
                .standalone_env
                .upgrade()
                .expect("Worker env was dropped.")
        };
        tokio::spawn(async move {
            standalone_env
                .withdraw_energy(&module_identity, energy_used.as_quanta())
                .await
                .unwrap();
        });
    }
}

struct Inner {
    standalone_env: Weak<StandaloneEnv>,
}

impl Inner {
    pub fn set_standalone_env(&mut self, worker_env: Arc<StandaloneEnv>) {
        self.standalone_env = Arc::downgrade(&worker_env);
    }

    /// To be used if we ever want to enable reducer budgets in Standalone
    fn _reducer_budget(&self, fingerprint: &EnergyMonitorFingerprint<'_>) -> EnergyQuanta {
        let standalone_env = self.standalone_env.upgrade().expect("Standalone env was dropped.");
        let balance = standalone_env
            .control_db
            .get_energy_balance(&fingerprint.module_identity)
            .unwrap()
            .unwrap_or(EnergyQuanta::ZERO);
        std::cmp::max(balance, EnergyQuanta::ZERO)
    }
}
