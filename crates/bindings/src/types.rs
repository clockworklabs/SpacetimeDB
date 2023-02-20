use std::ops::{Add, Sub};
use std::time::Duration;

use spacetimedb_lib::de::Deserialize;
use spacetimedb_lib::ser::Serialize;

use crate::rt::CURRENT_TIMESTAMP;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    pub(crate) micros_since_epoch: u64,
}

impl Timestamp {
    pub const UNIX_EPOCH: Self = Timestamp { micros_since_epoch: 0 };

    /// Panics if not in the context of a reducer
    pub fn now() -> Timestamp {
        assert!(CURRENT_TIMESTAMP.is_set(), "there is no current time in this context");
        CURRENT_TIMESTAMP.with(|x| *x)
    }

    pub fn elapsed(&self) -> Duration {
        Self::now()
            .duration_since(*self)
            .ok()
            .expect("timestamp for elapsed() is after current time")
    }

    pub fn duration_since(&self, earlier: Timestamp) -> Result<Duration, Duration> {
        let dur = Duration::from_micros(self.micros_since_epoch.abs_diff(earlier.micros_since_epoch));
        if earlier < *self {
            Ok(dur)
        } else {
            Err(dur)
        }
    }

    pub fn checked_add(&self, duration: Duration) -> Option<Self> {
        let micros = duration.as_micros().try_into().ok()?;
        let micros_since_epoch = self.micros_since_epoch.checked_add(micros)?;
        Some(Self { micros_since_epoch })
    }

    pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
        let micros = duration.as_micros().try_into().ok()?;
        let micros_since_epoch = self.micros_since_epoch.checked_sub(micros)?;
        Some(Self { micros_since_epoch })
    }
}

impl Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Duration) -> Self::Output {
        self.checked_add(rhs)
            .expect("overflow when adding duration to timestamp")
    }
}

impl Sub<Duration> for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: Duration) -> Self::Output {
        self.checked_sub(rhs)
            .expect("underflow when subtracting duration from timestamp")
    }
}

impl crate::SpacetimeType for Timestamp {
    fn get_schema() -> spacetimedb_lib::TypeDef {
        spacetimedb_lib::TypeDef::U64
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u64::deserialize(deserializer).map(|micros_since_epoch| Self { micros_since_epoch })
    }
}

impl Serialize for Timestamp {
    fn serialize<S: spacetimedb_lib::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.micros_since_epoch.serialize(serializer)
    }
}
