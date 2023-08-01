use core::fmt;
use sha3::{Digest, Keccak256};
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, AlgebraicType};

pub const HASH_SIZE: usize = 32;

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct Hash {
    pub data: [u8; HASH_SIZE],
}

impl_st!([] Hash, _ts => AlgebraicType::bytes());
impl_serialize!([] Hash, (self, ser) => self.data.serialize(ser));
impl_deserialize!([] Hash, de => Ok(Self { data: <_>::deserialize(de)? }));

impl Hash {
    const ABBREVIATION_LEN: usize = 16;

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

    pub fn to_hex(&self) -> String {
        hex::encode(self.data)
    }

    pub fn to_abbreviated_hex(&self) -> String {
        self.to_hex()[0..Hash::ABBREVIATION_LEN].to_owned()
    }

    pub fn as_slice(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub fn from_hex(hex: impl AsRef<[u8]>) -> Result<Self, hex::FromHexError> {
        hex::FromHex::from_hex(hex)
    }
}

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> Hash {
    let data: [u8; HASH_SIZE] = Keccak256::digest(bytes).into();
    Hash { data }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.data))
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
        spacetimedb_sats::ser::serde::serialize_to(self, serializer)
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Hash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        spacetimedb_sats::de::serde::deserialize_from(deserializer)
    }
}
