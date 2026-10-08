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
        if state.accounted_bytes + bytes <= self.config.hard_limit_bytes {
            state.accounted_bytes += bytes;
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
        state.accounted_bytes += bytes;
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

    #[test]
    fn permit_drop_returns_accounted_bytes() {
        let budget = ByteBudget::new(ByteBudgetConfig {
            low_water_bytes: 16,
            soft_limit_bytes: 16,
            hard_limit_bytes: 32,
        })
        .unwrap();
        assert_eq!(budget.usage().accounted_bytes(), 0);
        let permit = budget.acquire(8);
        assert_eq!(budget.usage().accounted_bytes(), 8);
        drop(permit);
        assert_eq!(budget.usage().accounted_bytes(), 0);
    }

    #[test]
    fn acquire_fails_over_hardlimit() {
        let budget = ByteBudget::new(ByteBudgetConfig {
            low_water_bytes: 16,
            soft_limit_bytes: 16,
            hard_limit_bytes: 32,
        })
        .unwrap();
        for x in [8, 16, 21, 32] {
            assert!(budget.acquire(x).is_ok());
        }
        assert!(matches!(
            budget.acquire(64),
            Err(BudgetExceeded {
                requested_bytes: 64,
                accounted_bytes: 0,
                hard_limit_bytes: 32
            })
        ));
    }

    #[test]
    fn force_acquire_succeeds_over_hardlimit() {
        let budget = ByteBudget::new(ByteBudgetConfig {
            low_water_bytes: 16,
            soft_limit_bytes: 16,
            hard_limit_bytes: 32,
        })
        .unwrap();
        let _permit = budget.force_acquire(64);
        assert_eq!(budget.usage().accounted_bytes(), 64);
    }
}
