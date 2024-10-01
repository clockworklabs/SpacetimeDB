use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct QueryTimer {
    start: Instant,
}

impl QueryTimer {
    pub fn total(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Default for QueryTimer {
    fn default() -> Self {
        QueryTimer { start: Instant::now() }
    }
}
