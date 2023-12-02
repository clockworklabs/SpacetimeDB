use spacetimedb_bindings_macro::{Deserialize, Serialize};
use spacetimedb_metrics::impl_prometheusvalue_string;
use spacetimedb_metrics::typed_prometheus::AsPrometheusLabel;
use spacetimedb_sats::hex::HexString;
use spacetimedb_sats::{hash, impl_st, AlgebraicType};
use std::{fmt, str::FromStr};

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

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct Identity {
    __identity_bytes: [u8; 32],
}

impl_st!([] Identity, _ts => Identity::get_type());
impl_prometheusvalue_string!(Identity);

impl Identity {
    /// Returns an `Identity` defined as the given `bytes` byte array.
    pub const fn from_byte_array(bytes: [u8; 32]) -> Self {
        Self {
            __identity_bytes: bytes,
        }
    }

    /// Returns an `Identity` defined as the given byte `slice`.
    pub fn from_slice(slice: &[u8]) -> Self {
        Self::from_byte_array(slice.try_into().unwrap())
    }

    #[doc(hidden)]
    pub fn __dummy() -> Self {
        Self::from_byte_array([0; 32])
    }

    pub fn get_type() -> AlgebraicType {
        AlgebraicType::product([("__identity_bytes", AlgebraicType::bytes())])
    }

    /// Returns a borrowed view of the byte array defining this `Identity`.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.__identity_bytes
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.__identity_bytes.to_vec()
    }

    pub fn to_hex(&self) -> HexString<32> {
        spacetimedb_sats::hex::encode(&self.__identity_bytes)
    }

    pub fn abbreviate(&self) -> &[u8; 8] {
        self.__identity_bytes[..8].try_into().unwrap()
    }

    pub fn to_abbreviated_hex(&self) -> HexString<8> {
        spacetimedb_sats::hex::encode(self.abbreviate())
    }

    pub fn from_hex(hex: impl AsRef<[u8]>) -> Result<Self, hex::FromHexError> {
        hex::FromHex::from_hex(hex)
    }

    pub fn from_hashing_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Identity::from_byte_array(hash::hash_bytes(bytes).data)
    }
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.to_hex())
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Identity").field(&self.to_hex()).finish()
    }
}

impl hex::FromHex for Identity {
    type Error = hex::FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let data = hex::FromHex::from_hex(hex)?;
        Ok(Identity { __identity_bytes: data })
    }
}

impl FromStr for Identity {
    type Err = <Self as hex::FromHex>::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Identity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        spacetimedb_sats::ser::serde::serialize_to(self.as_bytes(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Identity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(Identity::from_byte_array(arr))
    }
}
