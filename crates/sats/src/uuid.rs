use crate::timestamp::ClockGenerator;
use crate::{de::Deserialize, impl_st, ser::Serialize, AlgebraicType, AlgebraicValue};
use std::fmt;
use uuid::{Builder, Uuid as UUID};

#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct Uuid {
    __uuid__: u128,
}

impl Uuid {
    pub const NIL: Self = Self {
        __uuid__: UUID::nil().as_u128(),
    };

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub fn now_v7() -> Self {
        use crate::timestamp::Timestamp;
        use rand::RngCore;

        let mut bytes = [0u8; 10];
        rand::rng().fill_bytes(&mut bytes);
        let mut clock = Timestamp::now().into();
        Self::new_v7_from_timestamp(&mut clock, &bytes).unwrap()
    }

    pub fn new_v7(millis: u64, counter_random_bytes: &[u8; 10]) -> Self {
        Self {
            __uuid__: Builder::from_unix_timestamp_millis(millis, counter_random_bytes)
                .into_uuid()
                .as_u128(),
        }
    }

    pub fn new_v7_from_timestamp(clock: &mut ClockGenerator, counter_random_bytes: &[u8; 10]) -> anyhow::Result<Self> {
        let timestamp = clock.tick();
        let millis = timestamp
            .to_duration_since_unix_epoch()
            .map_err(|err| anyhow::anyhow!("cannot create v7 UUID from timestamp before Unix epoch: {err:?}"))?
            .as_millis()
            .try_into()?;
        Ok(Uuid::new_v7(millis, counter_random_bytes))
    }

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub fn new_v4() -> Self {
        Self {
            __uuid__: UUID::new_v4().as_u128(),
        }
    }

    pub fn new_v4_from_random_bytes(counter_random_bytes: [u8; 16]) -> Self {
        Self {
            __uuid__: Builder::from_random_bytes(counter_random_bytes).into_uuid().as_u128(),
        }
    }

    pub fn parse_str(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self {
            __uuid__: UUID::parse_str(s)?.as_u128(),
        })
    }
    pub fn to_uuid(self) -> UUID {
        UUID::from_u128(self.__uuid__)
    }

    pub fn from_u128(u: u128) -> Self {
        Self { __uuid__: u }
    }

    pub fn as_u128(&self) -> u128 {
        self.__uuid__
    }
}

impl_st!([] Uuid, AlgebraicType::uuid());

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uuid())
    }
}

impl From<Uuid> for AlgebraicValue {
    fn from(value: Uuid) -> Self {
        AlgebraicValue::product([value.as_u128().into()])
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GroundSpacetimeType;

    #[test]
    fn uuid_type_matches() {
        assert_eq!(AlgebraicType::uuid(), Uuid::get_type());
        assert!(Uuid::get_type().is_uuid());
        assert!(Uuid::get_type().is_special());
    }

    #[test]
    fn round_trip_uuid() {
        let u1 = Uuid::NIL;
        let s = u1.to_string();
        let u2 = Uuid::parse_str(&s).unwrap();
        assert_eq!(u1, u2);
        assert_eq!(u1.as_u128(), u2.as_u128());
        assert_eq!(u1.to_uuid(), u2.to_uuid());
        assert_eq!(s, u2.to_string());
    }
}
