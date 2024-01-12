use crate::StandaloneEnv;
use spacetimedb::energy::{EnergyMonitor, EnergyQuanta, ReducerBudget, ReducerFingerprint};
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
        assert!(!energy_used.get().is_negative());
        if energy_used.get() == 0 {
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
                .withdraw_energy(&module_identity, energy_used)
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
    fn _reducer_budget(&self, fingerprint: &ReducerFingerprint<'_>) -> EnergyQuanta {
        let standalone_env = self.standalone_env.upgrade().expect("Standalone env was dropped.");
        let balance = standalone_env
            .control_db
            .get_energy_balance(&fingerprint.module_identity)
            .unwrap()
            .unwrap_or(EnergyQuanta::ZERO);
        std::cmp::max(balance, EnergyQuanta::ZERO)
    }
}
