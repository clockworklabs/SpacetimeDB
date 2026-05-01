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

impl EnergyQuanta {
    pub const ZERO: Self = EnergyQuanta { quanta: 0 };

    #[inline]
    pub const fn new(quanta: u128) -> Self {
        Self { quanta }
    }

    #[inline]
    pub const fn get(&self) -> u128 {
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

/// A measure of the energy budget for a reducer or any callable function.
///
/// This unit is not directly convertible to `EnergyQuanta`. It is currently
/// 1:1 to wasmtime fuel, and we intend to treat it as representing a CPU
/// instruction.
#[derive(Copy, Clone, From, Add, Sub, AddAssign, SubAssign, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunctionBudget(u64);

impl FunctionBudget {
    /// We've generally assumed that 1 second of wasm runtime uses 2_000_000_000 fuel.
    /// Currently, 1 wasmtime fuel unit is equivalent to 1 wasm instructions. Assuming
    /// 1 wasm instruction compiles to 1 CPU instruction (which it doesn't), this implies
    /// a 1 instruction-per-cycle abstract machine with a CPU frequency of 2GHz.
    pub const PER_EXECUTION_SEC: Self = FunctionBudget(2_000_000_000);

    pub const PER_EXECUTION_NANOSEC: Self = Self(Self::PER_EXECUTION_SEC.0 / 1_000_000_000);

    /// Roughly 1 minute of runtime.
    pub const DEFAULT_BUDGET: Self = FunctionBudget(Self::PER_EXECUTION_SEC.0 * 60);

    pub const ZERO: Self = FunctionBudget(0);
    pub const MAX: Self = FunctionBudget(u64::MAX);

    pub const fn new(v: u64) -> Self {
        Self(v)
    }

    pub const fn get(&self) -> u64 {
        self.0
    }
}
