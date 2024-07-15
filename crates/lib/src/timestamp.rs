use std::time::{Duration, SystemTime};

use spacetimedb_sats::SpacetimeType;

/// A timestamp, as a number of microseconds since the UNIX epoch.
// Ideally this should be a transparent newtype,
// incl. `serde(transparent)` and some future `sats(transparent)` marker.
// Because `sats(transparent)` is not currently designed or implemented (as of 2024-06-20),
// we define this as a regular struct, which serializes as a `ProductValue`.
#[derive(SpacetimeType, Copy, Clone, PartialEq, Eq, Debug)]
#[sats(crate = spacetimedb_sats)]
pub struct Timestamp {
    pub microseconds: u64,
}

impl Timestamp {
    pub fn from_microseconds(microseconds: u64) -> Self {
        Timestamp { microseconds }
    }
    pub fn now() -> Self {
        Self::from_systemtime(SystemTime::now())
    }
    pub fn from_systemtime(systime: SystemTime) -> Self {
        let dur = systime.duration_since(SystemTime::UNIX_EPOCH).expect("hello, 1969");
        // UNIX_EPOCH + u64::MAX microseconds is in 586524 CE, so it's probably fine to cast
        Self {
            microseconds: dur.as_micros() as u64,
        }
    }
    pub fn to_systemtime(self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_micros(self.microseconds)
    }
    pub fn to_duration_from_now(self) -> Duration {
        self.to_systemtime()
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO)
    }
}
