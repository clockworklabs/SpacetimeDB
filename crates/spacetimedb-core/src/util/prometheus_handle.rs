use prometheus::{Histogram, HistogramVec};
use std::time::Instant;

/// An RAII-style handle for doing quick measurements of a single vertex Histogram value.
/// Time spent is measured at Drop, meaning the sample occurs regardless of how the owning scope
/// exits.
pub struct HistogramHandle {
    hist: &'static Histogram,
    start: Option<Instant>,
}
impl HistogramHandle {
    pub fn new(hist: &'static Histogram) -> Self {
        HistogramHandle { hist, start: None }
    }

    pub fn start(&mut self) {
        self.start = Some(Instant::now());
    }

    pub fn stop(&mut self) {
        if self.start.is_none() {
            return;
        };
        let duration = self.start.unwrap().elapsed();
        self.hist.observe(duration.as_micros() as f64);
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
    start: Option<Instant>,
}
impl HistogramVecHandle {
    pub fn new(hist: &'static HistogramVec, label_values: Vec<String>) -> Self {
        HistogramVecHandle {
            hist,
            label_values,
            start: None,
        }
    }

    pub fn start(&mut self) {
        self.start = Some(Instant::now());
    }

    pub fn stop(&mut self) {
        if self.start.is_none() {
            return;
        };
        let duration = self.start.unwrap().elapsed();
        let labels: Vec<&str> = self.label_values.as_slice().iter().map(|s| s.as_str()).collect();
        self.hist
            .with_label_values(labels.as_slice())
            .observe(duration.as_micros() as f64);
    }
}
impl Drop for HistogramVecHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
