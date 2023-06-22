//! Defines a `Timestamp` abstraction.

use std::ops::{Add, Sub};
use std::time::Duration;

use spacetimedb_lib::de::Deserialize;
use spacetimedb_lib::ser::Serialize;

scoped_tls::scoped_thread_local! {
    static CURRENT_TIMESTAMP: Timestamp
}

/// Set the current timestamp for the duration of the function `f`.
pub(crate) fn with_timestamp_set<R>(ts: Timestamp, f: impl FnOnce() -> R) -> R {
    CURRENT_TIMESTAMP.set(&ts, f)
}

/// A timestamp measured as micro seconds since the UNIX epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    /// The number of micro seconds since the UNIX epoch.
    pub(crate) micros_since_epoch: u64,
}

impl Timestamp {
    /// The timestamp 0 micro seconds since the UNIX epoch.
    pub const UNIX_EPOCH: Self = Timestamp { micros_since_epoch: 0 };

    /// Returns a timestamp of how many micros have passed right now since UNIX epoch.
    ///
    /// Panics if not in the context of a reducer.
    pub fn now() -> Timestamp {
        assert!(CURRENT_TIMESTAMP.is_set(), "there is no current time in this context");
        CURRENT_TIMESTAMP.with(|x| *x)
    }

    /// Returns how many micros have passed since the UNIX epoch as a `Duration`.
    pub fn elapsed(&self) -> Duration {
        Self::now()
            .duration_since(*self)
            .expect("timestamp for elapsed() is after current time")
    }

    /// Returns the absolute difference between this and an `earlier` timestamp as a `Duration`.
    ///
    /// Returns an error when `earlier >= self`.
    pub fn duration_since(&self, earlier: Timestamp) -> Result<Duration, Duration> {
        let dur = Duration::from_micros(self.micros_since_epoch.abs_diff(earlier.micros_since_epoch));
        if earlier < *self {
            Ok(dur)
        } else {
            Err(dur)
        }
    }

    /// Returns a timestamp with `duration` added to `self`.
    ///
    /// Returns `None` when a `u64` is overflowed.
    pub fn checked_add(&self, duration: Duration) -> Option<Self> {
        let micros = duration.as_micros().try_into().ok()?;
        let micros_since_epoch = self.micros_since_epoch.checked_add(micros)?;
        Some(Self { micros_since_epoch })
    }

    /// Returns a timestamp with `duration` subtracted from `self`.
    ///
    /// Returns `None` when a `u64` is overflowed.
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
    fn make_type<S: spacetimedb_lib::sats::typespace::TypespaceBuilder>(_ts: &mut S) -> spacetimedb_lib::AlgebraicType {
        spacetimedb_lib::AlgebraicType::U64
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
