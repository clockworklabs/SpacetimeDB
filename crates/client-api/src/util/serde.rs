/// Ser/De of [`std::time::Duration`] via the `humantime` crate.
///
/// Suitable for use with the `#[serde(with)]` annotation.
pub(crate) mod humantime_duration {
    use std::time::Duration;

    use ::serde::{Deserialize as _, Deserializer, Serialize as _, Serializer};

    pub fn serialize<S: Serializer>(duration: &Duration, ser: S) -> Result<S::Ok, S::Error> {
        humantime::format_duration(*duration).to_string().serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Duration, D::Error> {
        // TODO: `toml` chokes if we try to derserialize to `&str` here.
        let s = String::deserialize(de)?;
        humantime::parse_duration(&s).map_err(serde::de::Error::custom)
    }
}
