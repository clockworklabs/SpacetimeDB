#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub seed: u64,
}

impl RuntimeConfig {
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new(0)
    }
}
