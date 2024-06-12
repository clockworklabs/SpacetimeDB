use std::time::{Duration, SystemTime};

use spacetimedb_sats::SpacetimeType;

#[derive(SpacetimeType, Copy, Clone, PartialEq, Eq, Debug, serde::Serialize)]
#[sats(crate = spacetimedb_sats, transparent)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn now() -> Self {
        Self::from_systemtime(SystemTime::now())
    }
    pub fn from_systemtime(systime: SystemTime) -> Self {
        let dur = systime.duration_since(SystemTime::UNIX_EPOCH).expect("hello, 1969");
        // UNIX_EPOCH + u64::MAX microseconds is in 586524 CE, so it's probably fine to cast
        Self(dur.as_micros() as u64)
    }
    pub fn to_systemtime(self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_micros(self.0)
    }
    pub fn to_duration_from_now(self) -> Duration {
        self.to_systemtime()
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO)
    }
}
