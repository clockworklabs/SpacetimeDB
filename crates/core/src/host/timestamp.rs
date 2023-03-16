use std::time::{Duration, SystemTime};

#[derive(Copy, Clone, Debug)]
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

impl<'de> spacetimedb_sats::de::Deserialize<'de> for Timestamp {
    fn deserialize<D: spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u64::deserialize(deserializer).map(Self)
    }
}
impl spacetimedb_sats::ser::Serialize for Timestamp {
    fn serialize<S: spacetimedb_lib::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
