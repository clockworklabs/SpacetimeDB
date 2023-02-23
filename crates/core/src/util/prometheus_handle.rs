use prometheus::{Histogram, HistogramVec};
use std::time::{Duration, SystemTime};

/// An RAII-style handle for doing quick measurements of a single vertex Histogram value.
/// Time spent is measured at Drop, meaning the sample occurs regardless of how the owning scope
/// exits.
pub struct HistogramHandle {
    hist: &'static Histogram,
    pub start_instant: Option<SystemTime>,
}

impl HistogramHandle {
    pub fn new(hist: &'static Histogram) -> Self {
        HistogramHandle {
            hist,
            start_instant: None,
        }
    }

    pub fn start(&mut self) {
        self.start_instant = Some(SystemTime::now());
    }

    pub fn stop(&mut self) {
        self.hist.observe(self.elapsed().as_secs_f64());
    }

    pub fn elapsed(&self) -> Duration {
        let Some(start_instant) = self.start_instant else {
            return Duration::ZERO;
        };
        let duration = start_instant.elapsed();
        duration.unwrap_or(Duration::ZERO)
    }
}
impl Drop for HistogramHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// An RAII-style handle for doing quick measurements of a multi-vertex labelled histogram value.
pub struct HistogramVecHandle {
    hist: &'static HistogramVec,
    label_values: Vec<String>,
    pub start_instant: Option<SystemTime>,
}
impl HistogramVecHandle {
    pub fn new(hist: &'static HistogramVec, label_values: Vec<String>) -> Self {
        HistogramVecHandle {
            hist,
            label_values,
            start_instant: None,
        }
    }

    pub fn start(&mut self) {
        self.start_instant = Some(SystemTime::now());
    }

    pub fn stop(&mut self) {
        let labels: Vec<&str> = self.label_values.as_slice().iter().map(|s| s.as_str()).collect();
        self.hist
            .with_label_values(labels.as_slice())
            .observe(self.elapsed().as_secs_f64());
    }

    pub fn elapsed(&self) -> Duration {
        let Some(start_instant) = self.start_instant else {
            return Duration::ZERO;
        };
        let duration = start_instant.elapsed();
        duration.unwrap_or(Duration::ZERO)
    }
}
impl Drop for HistogramVecHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
