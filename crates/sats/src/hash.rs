use crate::hex::HexString;
use crate::{impl_deserialize, impl_serialize, impl_st, AlgebraicType};
use core::fmt;
use sha3::{Digest, Keccak256};
use spacetimedb_metrics::impl_prometheusvalue_string;
use spacetimedb_metrics::typed_prometheus::AsPrometheusLabel;

pub const HASH_SIZE: usize = 32;

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash)]
#[cfg_attr(any(test, feature = "proptest"), derive(proptest_derive::Arbitrary))]
pub struct Hash {
    pub data: [u8; HASH_SIZE],
}

impl_st!([] Hash, _ts => AlgebraicType::bytes());
impl_serialize!([] Hash, (self, ser) => self.data.serialize(ser));
impl_deserialize!([] Hash, de => Ok(Self { data: <_>::deserialize(de)? }));
impl_prometheusvalue_string!(Hash);

impl Hash {
    pub const ZERO: Self = Self { data: [0; HASH_SIZE] };

    pub fn from_arr(arr: &[u8; HASH_SIZE]) -> Self {
        Self { data: *arr }
    }

    pub fn from_slice(slice: &[u8]) -> Self {
        Self {
            data: slice.try_into().unwrap(),
        }
    }
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }

    pub fn to_hex(&self) -> HexString<32> {
        crate::hex::encode(&self.data)
    }

    pub fn abbreviate(&self) -> &[u8; 16] {
        self.data[..16].try_into().unwrap()
    }

    pub fn to_abbreviated_hex(&self) -> HexString<16> {
        crate::hex::encode(self.abbreviate())
    }

    pub fn as_slice(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub fn from_hex(hex: impl AsRef<[u8]>) -> Result<Self, hex::FromHexError> {
        hex::FromHex::from_hex(hex)
    }
}

#[tracing::instrument(skip_all)]
pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> Hash {
    let data: [u8; HASH_SIZE] = Keccak256::digest(bytes).into();
    Hash { data }
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

pub struct HashFromHexError(usize);

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
