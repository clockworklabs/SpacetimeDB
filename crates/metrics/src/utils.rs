use std::time::Instant;

use prometheus::{Histogram, IntGauge};

/// Decrements the inner [`IntGauge`] on drop.
pub struct GaugeInc {
    gauge: IntGauge,
}
impl Drop for GaugeInc {
    #[inline]
    fn drop(&mut self) {
        self.gauge.dec();
    }
}

/// Increment the given [`IntGauge`], and decrement it when the returned value goes out of scope.
#[inline]
pub fn inc_scope(gauge: &IntGauge) -> GaugeInc {
    gauge.inc();
    GaugeInc { gauge: gauge.clone() }
}

pub trait IntGaugeExt {
    fn inc_scope(&self) -> GaugeInc;
}

impl IntGaugeExt for IntGauge {
    fn inc_scope(&self) -> GaugeInc {
        inc_scope(self)
    }
}

/// A scope guard for a timer,
/// the total duration of which is written to a Histogram metric on drop.
pub struct TimerGuard {
    histogram: Histogram,
    timer: Instant,
}

impl Drop for TimerGuard {
    fn drop(&mut self) {
        self.histogram.observe(self.timer.elapsed().as_secs_f64());
    }
}

pub trait HistogramExt {
    fn with_timer(self, timer: Instant) -> TimerGuard;
}

impl HistogramExt for Histogram {
    fn with_timer(self, timer: Instant) -> TimerGuard {
        TimerGuard { histogram: self, timer }
    }
}
