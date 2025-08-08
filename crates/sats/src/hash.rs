use crate::hex::HexString;
use crate::{impl_deserialize, impl_serialize, impl_st, u256, AlgebraicType};
use core::fmt;
use sha3::{Digest, Keccak256};

pub const HASH_SIZE: usize = 32;

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash)]
#[cfg_attr(any(test, feature = "proptest"), derive(proptest_derive::Arbitrary))]
pub struct Hash {
    pub data: [u8; HASH_SIZE],
}

impl_st!([] Hash, AlgebraicType::U256);
impl_serialize!([] Hash, (self, ser) => u256::from_le_bytes(self.data).serialize(ser));
impl_deserialize!([] Hash, de => Ok(Self { data: <_>::deserialize(de).map(u256::to_le_bytes)? }));

#[cfg(feature = "metrics_impls")]
impl spacetimedb_metrics::typed_prometheus::AsPrometheusLabel for Hash {
    fn as_prometheus_str(&self) -> impl AsRef<str> + '_ {
        self.to_hex()
    }
}

impl Hash {
    pub const ZERO: Self = Self::from_byte_array([0; HASH_SIZE]);

    pub const fn from_byte_array(data: [u8; HASH_SIZE]) -> Self {
        Self { data }
    }

    pub fn from_u256(val: u256) -> Self {
        Self::from_byte_array(val.to_le_bytes())
    }

    pub fn to_u256(self) -> u256 {
        u256::from_le_bytes(self.data)
    }

    pub fn to_hex(&self) -> HexString<32> {
        crate::hex::encode(&self.data)
    }

    pub fn abbreviate(&self) -> &[u8; 16] {
        self.data[..16].try_into().unwrap()
    }

    pub fn from_hex(hex: impl AsRef<[u8]>) -> Result<Self, hex::FromHexError> {
        hex::FromHex::from_hex(hex)
    }
}

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> Hash {
    Hash::from_byte_array(Keccak256::digest(bytes).into())
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Hash").field(&format_args!("{self}")).finish()
    }
}

impl hex::FromHex for Hash {
    type Error = hex::FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let data = hex::FromHex::from_hex(hex)?;
        Ok(Hash { data })
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Hash {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        crate::ser::serde::serialize_to(self, serializer)
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Hash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::de::serde::deserialize_from(deserializer)
    }
}
