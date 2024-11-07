use derive_more::{Add, AddAssign, From, Sub, SubAssign};
use spacetimedb_sats::SpacetimeType;
use std::fmt;
use std::time::Duration;

/// [EnergyQuanta] represents an amount of energy in a canonical unit.
/// It represents the smallest unit of energy that can be used to pay for
/// a reducer invocation. We will likely refer to this unit as an "eV".
///
#[derive(SpacetimeType, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Add, Sub, AddAssign, SubAssign)]
#[sats(crate = spacetimedb_sats)]
pub struct EnergyQuanta {
    pub quanta: u128,
}

#[derive(Copy, Clone)]
#[repr(transparent)]
#[must_use]
/// Time taken while executing datastore operations, for which energy should be charged.
///
/// A transparent newtype around [`Duration`], to enable a `must_use`
/// annotation so that we don't forget to charge energy.
pub struct DatastoreComputeDuration(pub Duration);

impl DatastoreComputeDuration {
    pub fn from_micros(micros: u64) -> Self {
        Self(Duration::from_micros(micros))
    }
}

impl EnergyQuanta {
    pub const ZERO: Self = EnergyQuanta { quanta: 0 };

    #[inline]
    pub fn new(quanta: u128) -> Self {
        Self { quanta }
    }

    #[inline]
    pub fn get(&self) -> u128 {
        self.quanta
    }

    pub fn from_disk_usage(bytes_stored: u64, storage_period: Duration) -> Self {
        let bytes_stored = u128::from(bytes_stored);
        let sec = u128::from(storage_period.as_secs());
        let nsec = u128::from(storage_period.subsec_nanos());
        // bytes_stored * storage_period, but make it complicated. floats might be lossy for large
        // enough values, so instead we expand the multiplication to (b * trunc(dur) + b * frac(dur)),
        // in a way that preserves integer precision despite a division
        let energy = bytes_stored * sec + (bytes_stored * nsec) / 1_000_000_000;
        Self::new(energy)
    }

    const ENERGY_PER_MEM_BYTE_SEC: u128 = 100;

    pub fn from_memory_usage(bytes_stored: u64, storage_period: Duration) -> Self {
        let byte_seconds = Self::from_disk_usage(bytes_stored, storage_period).get();
        Self::new(byte_seconds * Self::ENERGY_PER_MEM_BYTE_SEC)
    }

    // TODO(energy): This should probably be dynamically specified by the server owner at startup,
    // as the price/value per time to operate a machine varies wildly depending on the specific hardware.
    const ENERGY_PER_DATASTORE_MICROSECOND: u128 = 100;

    pub fn from_datastore_compute_duration(compute_time: DatastoreComputeDuration) -> Self {
        let micros = compute_time.0.as_micros();
        Self::new(micros * Self::ENERGY_PER_DATASTORE_MICROSECOND)
    }
}

impl fmt::Display for EnergyQuanta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.quanta.fmt(f)?;
        f.write_str("eV")
    }
}

impl fmt::Debug for EnergyQuanta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// [`EnergyBalance`] same unit as [`EnergyQuanta`], but representing a user account's energy balance.
///
/// NOTE: This is represented by a signed integer, because it is possible
/// for a user's balance to go negative. This is allowable
/// for reasons of eventual consistency motivated by performance.
#[derive(Copy, Clone)]
pub struct EnergyBalance(i128);

impl EnergyBalance {
    pub const ZERO: Self = EnergyBalance(0);

    #[inline]
    pub fn new(v: i128) -> Self {
        Self(v)
    }

    #[inline]
    pub fn get(&self) -> i128 {
        self.0
    }

    /// Convert to [`EnergyQuanta`].
    ///
    /// If this balance is negative, this method returns an `Err` holding the amount
    /// negative that this balance is.
    pub fn to_energy_quanta(&self) -> Result<EnergyQuanta, EnergyQuanta> {
        if self.0.is_negative() {
            Err(EnergyQuanta::new(self.0.unsigned_abs()))
        } else {
            Ok(EnergyQuanta::new(self.0 as u128))
        }
    }

    pub fn checked_add_energy(self, energy: EnergyQuanta) -> Option<Self> {
        self.0.checked_add_unsigned(energy.get()).map(Self)
    }

    pub fn saturating_add_energy(&self, energy: EnergyQuanta) -> Self {
        Self(self.0.saturating_add_unsigned(energy.get()))
    }

    pub fn checked_sub_energy(self, energy: EnergyQuanta) -> Option<Self> {
        self.0.checked_sub_unsigned(energy.get()).map(Self)
    }

    pub fn saturating_sub_energy(&self, energy: EnergyQuanta) -> Self {
        Self(self.0.saturating_sub_unsigned(energy.get()))
    }
}

impl fmt::Display for EnergyBalance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)?;
        f.write_str("eV")
    }
}

impl fmt::Debug for EnergyBalance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EnergyBalance").field(self).finish()
    }
}

/// A measure of energy representing the energy budget for a reducer.
///
/// In contrast to [`EnergyQuanta`], this is represented by a 64-bit integer. This makes energy handling
/// for reducers easier, while still providing a unlikely-to-ever-be-reached maximum value (e.g. for wasmtime:
/// `(u64::MAX eV / 1000 eV/instruction) * 3 ns/instruction = 640 days`)
#[derive(Copy, Clone, From, Add, Sub)]
pub struct ReducerBudget(u64);

impl ReducerBudget {
    pub const DEFAULT_BUDGET: Self = ReducerBudget(1_000_000_000_000_000_000);

    pub const ZERO: Self = ReducerBudget(0);
    pub const MAX: Self = ReducerBudget(u64::MAX);

    pub fn new(v: u64) -> Self {
        Self(v)
    }

    pub fn get(&self) -> u64 {
        self.0
    }

    /// Convert from [`EnergyQuanta`]. Returns `None` if `energy` is too large to be represented.
    pub fn from_energy(energy: EnergyQuanta) -> Option<Self> {
        energy.get().try_into().ok().map(Self)
    }
}

impl From<ReducerBudget> for EnergyQuanta {
    fn from(value: ReducerBudget) -> Self {
        EnergyQuanta::new(value.0.into())
    }
}

impl fmt::Debug for ReducerBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ReducerBudget")
            .field(&EnergyQuanta::from(*self))
            .finish()
    }
}
