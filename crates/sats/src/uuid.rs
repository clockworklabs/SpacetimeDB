use std::cell::Cell;
use std::fmt;

use crate::timestamp::Timestamp;
use crate::{de::Deserialize, impl_st, ser::Serialize, AlgebraicType, AlgebraicValue};
use uuid::{Builder, Uuid as UUID, Variant};

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum Version {
    /// The "nil" UUID (all zeros).
    Nil = 0u8,
    /// Version 4: Random.
    V4 = 4,
    /// Version 7: Timestamp + counter + random.
    V7 = 7,
    /// The "max" (all ones) UUID.
    Max = 0xff,
}

/// A universally unique identifier (UUID).
///
/// Support for UUID [`Version::Nil`], [`Version::Max`], [`Version::V4`] (random) and [`Version::V7`] (timestamp + counter + random).
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct Uuid {
    __uuid__: u128,
}

impl Uuid {
    /// The nil UUID (all zeros).
    ///
    /// Example:
    ///
    /// ```
    /// # use spacetimedb_sats::uuid::Uuid;
    /// let uuid = Uuid::NIL;
    ///
    /// assert_eq!(
    ///     "00000000-0000-0000-0000-000000000000",
    ///     uuid.to_string(),
    /// );
    /// ```
    pub const NIL: Self = Self {
        __uuid__: UUID::nil().as_u128(),
    };

    /// The max UUID (all ones).
    ///
    /// Example:
    /// ```
    /// # use spacetimedb_sats::uuid::Uuid;
    /// let uuid = Uuid::MAX;
    ///
    /// assert_eq!(
    ///     "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ///     uuid.to_string(),
    /// );
    /// ```
    pub const MAX: Self = Self {
        __uuid__: UUID::max().as_u128(),
    };

    /// Create a UUID `v4` from explicit random bytes.
    ///
    /// This method assumes the bytes are already sufficiently random, it will only
    /// set the appropriate bits for the UUID version and variant.
    ///
    /// # Example
    /// ```
    /// # use spacetimedb_sats::uuid::Uuid;
    /// // Use the `ReducerContext::rng()` method to generate random bytes in reducers,
    /// // or call `ReducerContext::new_uuid_v4`
    /// let random_bytes = [0u8; 16];
    /// let uuid = Uuid::from_random_bytes_v4(random_bytes);
    ///
    /// assert_eq!(
    ///     "00000000-0000-4000-8000-000000000000",
    ///     uuid.to_string(),
    /// );
    /// ```
    pub fn from_random_bytes_v4(counter_random_bytes: [u8; 16]) -> Self {
        Self {
            __uuid__: Builder::from_random_bytes(counter_random_bytes).into_uuid().as_u128(),
        }
    }

    /// Generate a UUID `v7` using a monotonic counter from 0 to 2^31-1, a timestamp, and 4 random bytes.
    ///
    /// The counter will wrap around on overflow.
    ///
    /// The UUID `v7` is structured as follows:
    ///```ascii
    /// ┌───────────────────────────────────────────────┬───────────────────┐
    /// | B0  | B1  | B2  | B3  | B4  | B5              |         B6        |
    /// ├───────────────────────────────────────────────┼───────────────────┤
    /// |                 unix_ts_ms                    |      version 7    |
    /// └───────────────────────────────────────────────┴───────────────────┘
    /// ┌──────────────┬─────────┬──────────────────┬───────────────────────┐
    /// | B7           | B8      | B9  | B10 | B11  | B12 | B13 | B14 | B15 |
    /// ├──────────────┼─────────┼──────────────────┼───────────────────────┤
    /// | counter_high | variant |    counter_low   |        random         |
    /// └──────────────┴─────────┴──────────────────┴───────────────────────┘
    /// ```
    /// # Panics
    ///
    /// Panics if the counter value is negative, or the timestamp is before the Unix epoch.
    ///
    /// # Example
    ///```
    /// use spacetimedb_sats::uuid::Uuid;
    /// use spacetimedb_sats::timestamp::Timestamp;
    ///
    /// let now = Timestamp::from_micros_since_unix_epoch(1_686_000_000_000);
    /// let counter = std::cell::Cell::new(1);
    /// // Use the `ReducerContext::rng()` | `ProcedureContext::rng()` to generate random bytes,
    /// // or call `ReducerContext::new_uuid_v7()` / `ProcedureContext::new_uuid_v7()`
    /// let random_bytes = [0u8; 4];
    /// let uuid = Uuid::from_counter_v7(&counter, now, &random_bytes).unwrap();
    ///
    /// assert_eq!(
    ///     "0000647e-5180-7000-8000-000200000000",
    ///     uuid.to_string(),
    /// );
    /// ```
    pub fn from_counter_v7(counter: &Cell<u32>, now: Timestamp, random_bytes: &[u8; 4]) -> anyhow::Result<Self> {
        // Monotonic counter value (31 bits)
        let counter_val = counter.get();
        counter.set(counter_val.wrapping_add(1) & 0x7FFF_FFFF);

        let ts_ms = now
            .to_duration_since_unix_epoch()
            .expect("timestamp before unix epoch")
            .as_millis() as i64
            & 0xFFFFFFFFFFFF;

        let mut bytes = [0u8; 16];

        // unix_ts_ms (48 bits)
        bytes[0] = (ts_ms >> 40) as u8;
        bytes[1] = (ts_ms >> 32) as u8;
        bytes[2] = (ts_ms >> 24) as u8;
        bytes[3] = (ts_ms >> 16) as u8;
        bytes[4] = (ts_ms >> 8) as u8;
        bytes[5] = ts_ms as u8;

        // version & variant
        // bytes[6] = uuid::Version::SortRand;
        // bytes[8] = Variant::RFC4122

        // Counter bits
        bytes[7] = ((counter_val >> 23) & 0xFF) as u8;
        bytes[9] = ((counter_val >> 15) & 0xFF) as u8;
        bytes[10] = ((counter_val >> 7) & 0xFF) as u8;
        bytes[11] = ((counter_val & 0x7F) << 1) as u8;

        // Random bytes
        bytes[12] |= random_bytes[0] & 0x7F;
        bytes[13] = random_bytes[1];
        bytes[14] = random_bytes[2];
        bytes[15] = random_bytes[3];

        let uuid = Builder::from_bytes(bytes)
            .with_variant(Variant::RFC4122)
            .with_version(uuid::Version::SortRand)
            .into_uuid();

        Ok(Self {
            __uuid__: uuid.as_u128(),
        })
    }

    /// Extract the monotonic counter from a UUIDv7.
    #[cfg(test)]
    fn get_counter(&self) -> i32 {
        let bytes: [u8; 16] = self.__uuid__.to_be_bytes();

        let high = bytes[7] as u32; // bits 30..23
        let mid1 = bytes[9] as u32; // bits 22..15
        let mid2 = bytes[10] as u32; // bits 14..7
        let low = (bytes[11] as u32) >> 1; // bits 6..0

        // reconstruct 31-bit counter
        ((high << 23) | (mid1 << 15) | (mid2 << 7) | low) as i32
    }

    /// Parse a UUID from a string representation.
    ///
    /// Any of the formats generated by this module (simple, hyphenated, urn,
    /// Microsoft GUID) are supported by this parsing function.
    ///
    /// # Example
    /// ```
    /// # use spacetimedb_sats::uuid::Uuid;
    /// let s = "01888d6e-5c00-7000-8000-000000000000";
    /// let uuid = Uuid::parse_str(s).unwrap();
    ///
    /// assert_eq!(
    ///     s,
    ///     uuid.to_string(),
    /// );
    /// ```
    pub fn parse_str(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self {
            __uuid__: UUID::parse_str(s)?.as_u128(),
        })
    }

    /// Returns the version of the UUID.
    ///
    /// This represents the algorithm used to generate the value.
    /// If the version field doesn't contain a recognized version then `None`
    /// is returned.
    pub fn get_version(&self) -> Option<Version> {
        match self.to_uuid().get_version() {
            Some(uuid::Version::Nil) => Some(Version::Nil),
            Some(uuid::Version::Random) => Some(Version::V4),
            Some(uuid::Version::SortRand) => Some(Version::V7),
            Some(uuid::Version::Max) => Some(Version::Max),
            _ => None,
        }
    }

    #[cfg(test)]
    fn get_variant(&self) -> Variant {
        self.to_uuid().get_variant()
    }

    /// Convert to the `uuid` crate's `Uuid` type.
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
    use crate::timestamp::Timestamp;
    use crate::GroundSpacetimeType;
    use rand::RngCore;

    #[test]
    fn uuid_type_matches() {
        assert_eq!(AlgebraicType::uuid(), Uuid::get_type());
        assert!(Uuid::get_type().is_uuid());
        assert!(Uuid::get_type().is_special());
    }

    #[test]
    fn round_trip() {
        let u1 = Uuid::NIL;
        let s = u1.to_string();
        let u2 = Uuid::parse_str(&s).unwrap();
        assert_eq!(u1, u2);
        assert_eq!(u1.as_u128(), u2.as_u128());
        assert_eq!(u1.to_uuid(), u2.to_uuid());
        assert_eq!(s, u2.to_string());
    }

    #[test]
    fn to_string() {
        for u in [
            Uuid::NIL,
            Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10_u128),
            Uuid::MAX,
        ] {
            let s = u.to_string();
            let u2 = Uuid::parse_str(&s).unwrap();
            assert_eq!(u, u2);
        }
    }

    #[test]
    fn version() {
        let u_nil = Uuid::NIL;
        assert_eq!(u_nil.get_version(), Some(Version::Nil));

        let u_max = Uuid::MAX;
        assert_eq!(u_max.get_version(), Some(Version::Max));

        assert_eq!(0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF, u128::MAX);

        let u_v4 = Uuid::from_random_bytes_v4([0u8; 16]);
        assert_eq!(u_v4.get_version(), Some(Version::V4));

        let counter = Cell::new(0);
        let ts = Timestamp::from_micros_since_unix_epoch(1_686_000_000_000);
        let u_v7 = Uuid::from_counter_v7(&counter, ts, &[0u8; 4]).unwrap();
        assert_eq!(u_v7.get_version(), Some(Version::V7));
    }

    #[test]
    fn wrap_around() {
        // Check wraparound behavior
        let counter = Cell::new(u32::MAX);
        let ts = Timestamp::now();
        let _u1 = Uuid::from_counter_v7(&counter, ts, &[0u8; 4]).unwrap();
        assert_eq!(0, counter.get());
    }

    #[test]
    #[should_panic(expected = "timestamp before unix epoch")]
    fn negative_timestamp_panics() {
        let counter = Cell::new(0);
        let ts = Timestamp::from_micros_since_unix_epoch(-1);
        let _u = Uuid::from_counter_v7(&counter, ts, &[0u8; 4]).unwrap();
    }

    #[test]
    fn ordered() {
        let u1 = Uuid::from_u128(1);
        let u2 = Uuid::from_u128(2);
        assert!(u1 < u2);
        assert!(u2 > u1);
        assert_eq!(u1, u1);
        assert_ne!(u1, u2);
        // Check we start from zero
        let counter = Cell::new(0);
        let ts = Timestamp::now();
        let u_start = Uuid::from_counter_v7(&counter, ts, &[0u8; 4]).unwrap();
        assert_eq!(u_start.get_counter(), 0);
        // Check ordering over many UUIDs up to the max counter value
        let total = 10_000_000;
        let counter = Cell::new(u32::MAX - total);
        let ts = Timestamp::now();
        let uuids = (0..total)
            .map(|_| {
                let mut bytes = [0u8; 4];
                rand::rng().fill_bytes(&mut bytes);
                Uuid::from_counter_v7(&counter, ts, &bytes).unwrap()
            })
            .collect::<Vec<Uuid>>();

        for (pos, pair) in uuids.windows(2).enumerate() {
            assert_eq!(pair[0].get_version(), Some(Version::V7));
            assert_eq!(pair[0].get_variant(), Variant::RFC4122);
            assert!(
                pair[0] < pair[1],
                "UUIDs are not ordered at {pos}: {} !< {}",
                pair[0],
                pair[1]
            );

            assert!(
                pair[0].get_counter() < pair[1].get_counter(),
                "UUID counters are not ordered at {pos}: {} !< {}",
                pair[0].get_counter(),
                pair[1].get_counter()
            );
        }
    }
}
