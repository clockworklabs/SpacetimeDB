use std::fmt;

use sats::{impl_deserialize, impl_serialize, impl_st};

use crate::sats::{self, de, ser};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuthCtx {
    pub owner: Identity,
    pub caller: Identity,
}

impl AuthCtx {
    pub fn new(owner: Identity, caller: Identity) -> Self {
        Self { owner, caller }
    }
    /// For when the owner == caller
    pub fn for_current(owner: Identity) -> Self {
        Self { owner, caller: owner }
    }
    /// WARNING: Use this only for simple test were the `auth` don't matter
    pub fn for_testing() -> Self {
        AuthCtx {
            owner: Identity::__dummy(),
            caller: Identity::__dummy(),
        }
    }
}

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct Identity {
    pub data: [u8; 32],
}

impl Identity {
    #[doc(hidden)]
    pub fn __dummy() -> Self {
        Self { data: [0; 32] }
    }
}

impl_st!([] Identity, _ts => sats::AlgebraicType::bytes());
impl_serialize!([] Identity, (self, ser) => self.data.serialize(ser));
impl_deserialize!([] Identity, de => Ok(Self { data: <_>::deserialize(de)? }));

impl Identity {
    const ABBREVIATION_LEN: usize = 16;

    pub fn from_arr(arr: &[u8; 32]) -> Self {
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
        self.to_hex()[0..Identity::ABBREVIATION_LEN].to_owned()
    }

    pub fn as_slice(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub fn from_hex(hex: impl AsRef<[u8]>) -> Result<Self, hex::FromHexError> {
        hex::FromHex::from_hex(hex)
    }

    pub fn from_hashing_bytes(bytes: impl AsRef<[u8]>) -> Self {
        let hash = crate::hash::hash_bytes(bytes);
        Identity { data: hash.data }
    }
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.data))
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Identity").field(&format_args!("{self}")).finish()
    }
}

impl hex::FromHex for Identity {
    type Error = hex::FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let data = hex::FromHex::from_hex(hex)?;
        Ok(Identity { data })
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Identity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ser::serde::serialize_to(self, serializer)
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Identity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        de::serde::deserialize_from(deserializer)
    }
}
