use derive_more::{Add, AddAssign, From, Sub, SubAssign};
use std::fmt;
use std::time::Duration;

use spacetimedb_lib::{Hash, Identity};

use crate::messages::control_db::Database;

/// [EnergyQuanta] represents an amount of energy in a canonical unit.
/// It represents the smallest unit of energy that can be used to pay for
/// a reducer invocation. We will likely refer to this unit as an "eV".
///
/// NOTE: This is represented by a signed integer, because it is possible
/// for a user's balance to go negative. This is allowable
/// for reasons of eventual consistency motivated by performance.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Add, Sub, AddAssign, SubAssign)]
pub struct EnergyQuanta(i128);

impl EnergyQuanta {
    pub const ZERO: Self = EnergyQuanta(0);

    #[inline]
    pub fn new(v: i128) -> Self {
        Self(v)
    }

    #[inline]
    pub fn get(&self) -> i128 {
        self.0
    }

    pub fn from_disk_usage(bytes_stored: u64, storage_period: Duration) -> Self {
        // TODO: this line is lossy if bytes_stored is >1.6 PiB. do we ever care about that case?
        let energy = bytes_stored as f64 * storage_period.as_secs_f64();
        Self(energy as i128)
    }
}

impl fmt::Display for EnergyQuanta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)?;
        f.write_str("eV")
    }
}

impl fmt::Debug for EnergyQuanta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// A measure of energy representing the energy budget for a reducer.
///
/// In contrast to [`ReducerEnergy`], this is represented by an unsigned 64-bit integer. This makes energy handling
/// for reducers easier, while still providing a unlikely-to-ever-be-reached maximum value (e.g. for wasmtime:
/// `(u64::MAX eV / 1000 eV/instruction) * 3 ns/instruction = 640 days`)
#[derive(Copy, Clone, From, Add, Sub)]
pub struct ReducerBudget(u64);

impl ReducerBudget {
    pub const DEFAULT_BUDGET: Self = ReducerBudget(1_000_000_000_000_000_000);

    pub fn new(v: u64) -> Self {
        Self(v)
    }

    pub fn get(&self) -> u64 {
        self.0
    }
}

impl From<ReducerBudget> for EnergyQuanta {
    fn from(value: ReducerBudget) -> Self {
        EnergyQuanta(value.0.into())
    }
}

impl fmt::Debug for ReducerBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ReducerEnergy")
            .field(&EnergyQuanta(self.0.into()))
            .finish()
    }
}

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
    fn record_disk_usage(&self, database: &Database, instance_id: u64, disk_usage: u64, period: Duration);
}

// what would the module do with this information?
// pub enum EnergyRecordResult {
//     Continue,
//     Exhausted { quanta_over_budget: u64 },
// }

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

    fn record_disk_usage(&self, _database: &Database, _instance_id: u64, _disk_usage: u64, _period: Duration) {}
}
