use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct QueryTimer {
    start: Instant,
    execution: Duration,
}

impl QueryTimer {
    pub fn finish_execution(&mut self) {
        self.execution = self.start.elapsed()
    }

    pub fn total(&self) -> Duration {
        self.execution
    }
}

impl Default for QueryTimer {
    fn default() -> Self {
        let start = Instant::now();
        QueryTimer {
            start,
            execution: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Timed<T> {
    pub of: T,
    pub timer: QueryTimer,
}

impl<T> From<Timed<T>> for (T, QueryTimer) {
    fn from(value: Timed<T>) -> Self {
        (value.of, value.timer)
    }
}

impl<T> Timed<T> {
    pub fn new(timer: QueryTimer, of: T) -> Self {
        Self { of, timer }
    }
}
