use prometheus::IntGauge;

pub struct GaugeInc {
    gauge: IntGauge,
}
impl Drop for GaugeInc {
    #[inline]
    fn drop(&mut self) {
        self.gauge.dec()
    }
}

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
