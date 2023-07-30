use spacetimedb::host::{EnergyDiff, EnergyMonitor, EnergyMonitorFingerprint, EnergyQuanta};
use spacetimedb_client_api::ControlNodeDelegate;
use std::{time::Duration, sync::{Arc, Weak, Mutex}};
use crate::StandaloneEnv;

pub(crate) struct StandaloneEnergyMonitor {
    inner: Arc<Mutex<Inner>>,
}

impl StandaloneEnergyMonitor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                standalone_env: Weak::new()
            }))
        }
    }

    pub fn set_standalone_env(&self, standalone_env: Arc<StandaloneEnv>) {
        self.inner.lock().unwrap().set_standalone_env(standalone_env);
    }
}

impl EnergyMonitor for StandaloneEnergyMonitor {
    fn reducer_budget(&self, fingerprint: &EnergyMonitorFingerprint<'_>) -> EnergyQuanta {
        self.inner.lock().unwrap().reducer_budget(fingerprint)
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
        let standalone_env = { self.inner.lock().unwrap().standalone_env.upgrade().expect("Worker env was dropped.") };
        tokio::spawn(async move {
            standalone_env.withdraw_energy(&module_identity, energy_used.as_quanta()).await.unwrap();
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

    fn reducer_budget(&self, fingerprint: &EnergyMonitorFingerprint<'_>) -> EnergyQuanta {
        let standalone_env = self.standalone_env.upgrade().expect("Standalone env was dropped.");
        let balance = standalone_env.control_db.get_energy_balance(&fingerprint.module_identity).unwrap().unwrap_or(EnergyQuanta(0));
        EnergyQuanta(i128::max(balance.0, 0))
    }
}

