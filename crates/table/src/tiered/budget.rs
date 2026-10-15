use std::sync::{Arc, Mutex};

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
}

impl ByteBudget {
    pub(super) fn is_unlimited(&self) -> bool {
        self.config.hard_limit_bytes == u64::MAX
    }

    pub fn new(config: ByteBudgetConfig) -> Result<Self, ConfigError> {
        config.validate_then(|config| Self {
            state: Arc::new(Mutex::new(ByteBudgetState { accounted_bytes: 0 })),
            config,
        })
    }

    pub fn unlimited() -> Self {
        Self::try_from(ByteBudgetConfig::unlimited()).unwrap()
    }

    pub fn usage(&self) -> ByteBudgetUsage {
        ByteBudgetUsage {
            accounted_bytes: self.state.lock().unwrap().accounted_bytes,
        }
    }

    pub fn acquire(&self, bytes: u64) -> Result<BudgetPermit, BudgetExceeded> {
        let mut state = self.state.lock().unwrap();
        if let Some(accounted_bytes) = state.accounted_bytes.checked_add(bytes)
            && accounted_bytes <= self.config.hard_limit_bytes
        {
            state.accounted_bytes = accounted_bytes;
            Ok(BudgetPermit {
                state: self.state.clone(),
                bytes,
            })
        } else {
            Err(BudgetExceeded {
                requested_bytes: bytes,
                accounted_bytes: state.accounted_bytes,
                hard_limit_bytes: self.config.hard_limit_bytes,
            })
        }
    }

    pub(super) fn force_acquire(&self, bytes: u64) -> BudgetPermit {
        let mut state = self.state.lock().unwrap();
        state.accounted_bytes = state
            .accounted_bytes
            .checked_add(bytes)
            .expect("accounted bytes overflow");
        BudgetPermit {
            state: self.state.clone(),
            bytes,
        }
    }
}

impl TryFrom<ByteBudgetConfig> for ByteBudget {
    type Error = ConfigError;

    fn try_from(config: ByteBudgetConfig) -> Result<Self, Self::Error> {
        ByteBudget::new(config)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ByteBudgetConfig {
    pub low_water_bytes: u64,
    pub soft_limit_bytes: u64,
    pub hard_limit_bytes: u64,
}

impl ByteBudgetConfig {
    pub const fn unlimited() -> Self {
        Self {
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
        let mut state = self.state.lock().unwrap();
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
            drop(permit);
            prop_assert_eq!(budget.usage().accounted_bytes(), 0);
        }

        #[test]
        fn interleaved_acquire_and_drop_preserve_usage_and_observe_hard_limit(
            hard_limit_bytes in 0_u64..1_000_000,
            operations in vec(operation(), 0..100),
        ) {
            let budget = ByteBudget::new(ByteBudgetConfig {
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
        fn force_acquire_can_exceed_hard_limit(
            hard_limit_bytes in 0_u64..1_000_000,
            excess_bytes in 1_u64..1_000_000,
        ) {
            let budget = ByteBudget::new(ByteBudgetConfig {
                low_water_bytes: 0,
                soft_limit_bytes: hard_limit_bytes,
                hard_limit_bytes,
            }).unwrap();
            let forced_bytes = hard_limit_bytes + excess_bytes;

            let permit = budget.force_acquire(forced_bytes);

            prop_assert!(budget.usage().accounted_bytes() > hard_limit_bytes);
            prop_assert_eq!(budget.usage().accounted_bytes(), forced_bytes);
            drop(permit);
            prop_assert_eq!(budget.usage().accounted_bytes(), 0);
        }
    }
}
