use spacetimedb_memory_usage::MemoryUsage;

use super::metrics::{CacheTier, Metrics};
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("memory limit exceeded")]
pub struct BudgetExceeded {
    pub requested_bytes: u64,
    pub accounted_bytes: u64,
    pub hard_limit_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid configuration")]
    InvalidBudgetOrder(ByteBudgetConfig),
}

#[derive(Clone, Debug)]
pub struct ByteBudget {
    state: Arc<Mutex<ByteBudgetState>>,
    config: ByteBudgetConfig,
    pub(super) metrics: Metrics,
}

impl ByteBudget {
    pub(super) fn is_unlimited(&self) -> bool {
        self.config.hard_limit_bytes == u64::MAX
    }

    pub fn new(config: ByteBudgetConfig) -> Result<Self, ConfigError> {
        config.validate_then(|config| Self {
            state: Arc::new(Mutex::new(ByteBudgetState {
                accounted_bytes: 0,
                hard_limit_suspensions: 0,
            })),
            config,
            metrics: Default::default(),
        })
    }

    pub fn unlimited() -> Self {
        Self::try_from(ByteBudgetConfig::unlimited()).unwrap()
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn config(&self) -> &ByteBudgetConfig {
        &self.config
    }

    pub fn usage(&self) -> ByteBudgetUsage {
        ByteBudgetUsage {
            accounted_bytes: self.state.lock().accounted_bytes,
        }
    }

    pub fn acquire(&self, bytes: u64) -> Result<BudgetPermit, BudgetExceeded> {
        self.acquire_with_exemption(bytes, false)
    }

    /// Charge an allocation, applying the same exemption policy to system
    /// allocations and recovery. Accounting overflow is always rejected.
    pub(super) fn acquire_with_exemption(
        &self,
        bytes: u64,
        allocation_exempt: bool,
    ) -> Result<BudgetPermit, BudgetExceeded> {
        let mut state = self.state.lock();
        if let Some(accounted_bytes) = state.accounted_bytes.checked_add(bytes)
            && (allocation_exempt
                || state.hard_limit_suspensions > 0
                || accounted_bytes <= self.config.hard_limit_bytes)
        {
            state.accounted_bytes = accounted_bytes;
            Ok(BudgetPermit {
                state: self.state.clone(),
                bytes,
            })
        } else {
            let error = BudgetExceeded {
                requested_bytes: bytes,
                accounted_bytes: state.accounted_bytes,
                hard_limit_bytes: self.config.hard_limit_bytes,
            };
            // Metrics callbacks may inspect this budget or unwind. Neither
            // should happen while the accounting lock is held.
            drop(state);
            self.metrics.budget_rejected(self.config.tier);
            Err(error)
        }
    }

    /// Run a synchronous recovery operation with the hard limit suspended.
    ///
    /// Acquisitions still count toward usage, including those from budget clones.
    /// The suspension ends before this call returns or unwinds. The caller must
    /// exclude ordinary database access for the duration of the operation.
    pub fn with_hard_limit_suspended<R>(&self, operation: impl FnOnce() -> R) -> R {
        let _suspension = self.suspend_hard_limit();
        operation()
    }

    fn suspend_hard_limit(&self) -> HardLimitSuspension {
        let mut state = self.state.lock();
        state.hard_limit_suspensions = state
            .hard_limit_suspensions
            .checked_add(1)
            .expect("hard limit suspensions overflow");
        HardLimitSuspension {
            state: self.state.clone(),
        }
    }

    pub fn is_hard_limit_suspended(&self) -> bool {
        self.state.lock().hard_limit_suspensions > 0
    }
}

/// Keeps the shared budget's hard limit suspended until dropped.
#[derive(Debug)]
struct HardLimitSuspension {
    state: Arc<Mutex<ByteBudgetState>>,
}

impl Drop for HardLimitSuspension {
    fn drop(&mut self) {
        let mut state = self.state.lock();
        state.hard_limit_suspensions = state
            .hard_limit_suspensions
            .checked_sub(1)
            .expect("hard limit suspensions underflow");
    }
}

impl TryFrom<ByteBudgetConfig> for ByteBudget {
    type Error = ConfigError;

    fn try_from(config: ByteBudgetConfig) -> Result<Self, Self::Error> {
        ByteBudget::new(config)
    }
}

impl MemoryUsage for ByteBudget {
    fn heap_usage(&self) -> usize {
        let refcount = 2 * size_of::<usize>();
        refcount + size_of::<Mutex<ByteBudgetState>>()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ByteBudgetConfig {
    pub tier: CacheTier,
    pub low_water_bytes: u64,
    pub soft_limit_bytes: u64,
    pub hard_limit_bytes: u64,
}

impl ByteBudgetConfig {
    pub const fn unlimited() -> Self {
        Self {
            tier: CacheTier::Memory,
            low_water_bytes: u64::MAX,
            soft_limit_bytes: u64::MAX,
            hard_limit_bytes: u64::MAX,
        }
    }

    fn validate_then<T>(self, f: impl FnOnce(Self) -> T) -> Result<T, ConfigError> {
        if self.low_water_bytes <= self.soft_limit_bytes && self.soft_limit_bytes <= self.hard_limit_bytes {
            Ok(f(self))
        } else {
            Err(ConfigError::InvalidBudgetOrder(self))
        }
    }
}

#[derive(Debug)]
pub struct ByteBudgetState {
    accounted_bytes: u64,
    hard_limit_suspensions: u32,
}

impl ByteBudgetUsage {
    pub fn accounted_bytes(&self) -> u64 {
        self.accounted_bytes
    }
}

#[derive(Debug)]
pub struct BudgetPermit {
    state: Arc<Mutex<ByteBudgetState>>,
    bytes: u64,
}

impl Drop for BudgetPermit {
    fn drop(&mut self) {
        let mut state = self.state.lock();
        state.accounted_bytes = state
            .accounted_bytes
            .checked_sub(self.bytes)
            .expect("accounted bytes underflow");
    }
}

pub struct ByteBudgetUsage {
    pub accounted_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{collection::vec, prelude::*};

    #[derive(Clone, Copy, Debug)]
    enum Operation {
        Acquire(u64),
        Drop(u8),
    }

    fn operation() -> impl Strategy<Value = Operation> {
        prop_oneof![
            3 => (0u64..100_000).prop_map(Operation::Acquire),
            1 => any::<u8>().prop_map(Operation::Drop)
        ]
    }

    proptest! {
        #[test]
        fn configuration_is_valid_exactly_when_limits_are_ordered(
            low_water_bytes: u64,
            soft_limit_bytes: u64,
            hard_limit_bytes: u64,
        ) {
            let config = ByteBudgetConfig {
                tier: CacheTier::Memory,
                low_water_bytes,
                soft_limit_bytes,
                hard_limit_bytes,
            };
            let limits_are_ordered = low_water_bytes <= soft_limit_bytes
                && soft_limit_bytes <= hard_limit_bytes;

            prop_assert_eq!(ByteBudget::new(config).is_ok(), limits_are_ordered);
        }

        #[test]
        fn acquisitions_update_the_usage_without_exceeding_the_hard_limit(
            hard_limit_bytes in 0_u64..1_000_000,
            requests in vec(0_u64..100_000, 0..100),
        ) {
            let budget = ByteBudget::new(ByteBudgetConfig {
                tier: CacheTier::Memory,
                low_water_bytes: 0,
                soft_limit_bytes: 0,
                hard_limit_bytes,
            }).unwrap();
            let mut accounted_bytes = 0_u64;
            let mut permits = Vec::new();

            for requested_bytes in requests {
                let before = budget.usage().accounted_bytes();
                prop_assert_eq!(before, accounted_bytes);
                match budget.acquire(requested_bytes) {
                    Ok(permit) => {
                        accounted_bytes += requested_bytes;
                        permits.push(permit);
                    }
                    Err(error) => {
                        prop_assert!(accounted_bytes + requested_bytes > hard_limit_bytes);
                        prop_assert_eq!(error.requested_bytes, requested_bytes);
                        prop_assert_eq!(error.accounted_bytes, accounted_bytes);
                        prop_assert_eq!(error.hard_limit_bytes, hard_limit_bytes);
                    }
                }
                prop_assert_eq!(budget.usage().accounted_bytes(), accounted_bytes);
            }

            drop(permits);
            prop_assert_eq!(budget.usage().accounted_bytes(), 0);
        }

        #[test]
        fn acquisition_rejects_counter_overflow(accounted_bytes in 1_u64..=u64::MAX) {
            let budget = ByteBudget::unlimited();
            let permit = budget.acquire(accounted_bytes).unwrap();
            let requested_bytes = u64::MAX - accounted_bytes + 1;

            let error = budget.acquire(requested_bytes).unwrap_err();

            prop_assert_eq!(error.accounted_bytes, accounted_bytes);
            prop_assert_eq!(budget.usage().accounted_bytes(), accounted_bytes);

            // Suspending the limit must not suspend accounting overflow checks.
            let error = budget.with_hard_limit_suspended(|| budget.acquire(requested_bytes).unwrap_err());
            prop_assert_eq!(error.accounted_bytes, accounted_bytes);
            prop_assert_eq!(budget.usage().accounted_bytes(), accounted_bytes);
            drop(permit);
            prop_assert_eq!(budget.usage().accounted_bytes(), 0);
        }

        #[test]
        fn interleaved_acquire_and_drop_preserve_usage_and_observe_hard_limit(
            hard_limit_bytes in 0_u64..1_000_000,
            operations in vec(operation(), 0..100),
        ) {
            let budget = ByteBudget::new(ByteBudgetConfig {
                tier: CacheTier::Memory,
                low_water_bytes: 0,
                soft_limit_bytes: 0,
                hard_limit_bytes,
            }).unwrap();
            let mut accounted_bytes = 0_u64;
            let mut permits = Vec::<(u64, Option<BudgetPermit>)>::new();

            for operation in operations {
                prop_assert_eq!(budget.usage().accounted_bytes(), accounted_bytes);
                match operation {
                    Operation::Acquire(requested_bytes) => match budget.acquire(requested_bytes) {
                        Ok(permit) => {
                            prop_assert!(accounted_bytes + requested_bytes <= hard_limit_bytes);
                            accounted_bytes += requested_bytes;
                            permits.push((requested_bytes, Some(permit)));
                        }
                        Err(error) => {
                            prop_assert!(accounted_bytes + requested_bytes > hard_limit_bytes);
                            prop_assert_eq!(error.requested_bytes, requested_bytes);
                            prop_assert_eq!(error.accounted_bytes, accounted_bytes);
                            prop_assert_eq!(error.hard_limit_bytes, hard_limit_bytes);
                        }
                    },
                    Operation::Drop(index) if !permits.is_empty() => {
                        let index = usize::from(index) % permits.len();
                        let (bytes, permit) = &mut permits[index];
                        if let Some(permit) = permit.take() {
                            accounted_bytes -= *bytes;
                            drop(permit);
                        }
                    }
                    Operation::Drop(_) => {}
                }
                prop_assert_eq!(budget.usage().accounted_bytes(), accounted_bytes);
                prop_assert!(accounted_bytes <= hard_limit_bytes);
            }

            drop(permits);
            prop_assert_eq!(budget.usage().accounted_bytes(), 0);
        }

        #[test]
        fn exempt_acquisition_can_exceed_hard_limit(
            hard_limit_bytes in 0_u64..1_000_000,
            excess_bytes in 1_u64..1_000_000,
        ) {
            let budget = ByteBudget::new(ByteBudgetConfig {
                tier: CacheTier::Memory,
                low_water_bytes: 0,
                soft_limit_bytes: hard_limit_bytes,
                hard_limit_bytes,
            }).unwrap();
            let exempt_bytes = hard_limit_bytes + excess_bytes;

            let permit = budget.acquire_with_exemption(exempt_bytes, true).unwrap();

            prop_assert!(budget.usage().accounted_bytes() > hard_limit_bytes);
            prop_assert_eq!(budget.usage().accounted_bytes(), exempt_bytes);
            drop(permit);
            prop_assert_eq!(budget.usage().accounted_bytes(), 0);
        }
    }
}
