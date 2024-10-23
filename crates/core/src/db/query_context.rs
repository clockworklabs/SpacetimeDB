use crate::energy::{EnergyMonitor, NullEnergyMonitor};
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use spacetimedb_lib::Identity;
use spacetimedb_sats::energy::QueryTimer;
use std::sync::Arc;

pub struct QueryContext {
    pub(crate) energy_monitor: Arc<dyn EnergyMonitor>,
    owner_identity: Identity,
    pub(crate) timer: QueryTimer,
    replica_id: u64,
}

impl QueryContext {
    pub fn new(energy_monitor: Arc<dyn EnergyMonitor>, replica_id: u64, owner_identity: Identity) -> Self {
        Self {
            energy_monitor,
            owner_identity,
            timer: Default::default(),
            replica_id,
        }
    }

    pub fn for_testing() -> Self {
        Self::new(Arc::new(NullEnergyMonitor), 0, Identity::default())
    }

    pub fn from_subscriptions(subs: &ModuleSubscriptions) -> Self {
        Self::new(subs.energy_monitor.clone(), subs.replica_id, subs.owner_identity)
    }

    pub fn record_query_energy(&self) {
        self.energy_monitor
            .record_query_energy(self.owner_identity, self.replica_id, self.timer.total());
    }
}
