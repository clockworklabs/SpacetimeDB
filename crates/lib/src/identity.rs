use crate::from_hex_pad;
use core::mem;
use spacetimedb_bindings_macro::{Deserialize, Serialize};
use spacetimedb_sats::hex::HexString;
use spacetimedb_sats::{hash, impl_st, u256, AlgebraicType, AlgebraicValue};
use std::{fmt, str::FromStr};

pub type RequestId = u32;

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

/// An identifier for something interacting with the database.
///
/// This is a special type.
#[derive(Default, Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct Identity {
    __identity__: u256,
}

impl_st!([] Identity, AlgebraicType::identity());

#[cfg(feature = "metrics_impls")]
impl spacetimedb_metrics::typed_prometheus::AsPrometheusLabel for Identity {
    fn as_prometheus_str(&self) -> impl AsRef<str> + '_ {
        self.to_hex()
    }
}

use blake3;
impl Identity {
    pub const ZERO: Self = Self::from_u256(u256::ZERO);


    /// Returns an `Identity` defined as the given `bytes` byte array.
    pub const fn from_byte_array(bytes: [u8; 32]) -> Self {
        // SAFETY: The transmute is an implementation of `u256::from_ne_bytes`,
        // but works in a const context.
        Self::from_u256(u256::from_le(unsafe { mem::transmute(bytes) }))
    }

    /// Converts `__identity__: u256` to `Identity`.
    pub const fn from_u256(__identity__: u256) -> Self {
        Self { __identity__ }
    }

    /// Converts this identity to an `u256`.
    pub const fn to_u256(&self) -> u256 {
        self.__identity__
    }

    /// Returns an `Identity` defined as the given byte `slice`.
    pub fn from_slice(slice: &[u8]) -> Self {
        Self::from_byte_array(slice.try_into().unwrap())
    }

    #[doc(hidden)]
    pub fn __dummy() -> Self {
        Self::ZERO
    }

    pub fn from_claims(issuer: &str, subject: &str) -> Self {
        let input = format!("{}|{}", issuer, subject);
        let first_hash = blake3::hash(input.as_bytes());
        let id_hash = &first_hash.as_bytes()[..26];
        let mut checksum_input = [0u8; 28];
        // TODO: double check this gets the right number...
        checksum_input[2..].copy_from_slice(id_hash);
        checksum_input[0] = 0xc2;
        checksum_input[1] = 0x00;
        let checksum_hash = &blake3::hash(&checksum_input);

        let mut final_bytes = [0u8; 32];
        final_bytes[0] = 0xc2;
        final_bytes[1] = 0x00;
        final_bytes[2..6].copy_from_slice(&checksum_hash.as_bytes()[..4]);
        final_bytes[6..].copy_from_slice(id_hash);
        Identity::from_byte_array(final_bytes)
    }

    /// Returns this `Identity` as a byte array.
    pub fn to_byte_array(&self) -> [u8; 32] {
        self.__identity__.to_le_bytes()
    }

    pub fn to_hex(&self) -> HexString<32> {
        spacetimedb_sats::hex::encode(&self.to_byte_array())
    }

    pub fn abbreviate(&self) -> [u8; 8] {
        self.to_byte_array()[..8].try_into().unwrap()
    }

    pub fn to_abbreviated_hex(&self) -> HexString<8> {
        spacetimedb_sats::hex::encode(&self.abbreviate())
    }

    pub fn from_hex(hex: impl AsRef<[u8]>) -> Result<Self, hex::FromHexError> {
        hex::FromHex::from_hex(hex)
    }

    pub fn from_hashing_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self::from_byte_array(hash::hash_bytes(bytes).data)
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
        from_hex_pad(hex).map(Identity::from_byte_array)
    }
}

impl FromStr for Identity {
    type Err = <Self as hex::FromHex>::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

impl From<Identity> for AlgebraicValue {
    fn from(value: Identity) -> Self {
        AlgebraicValue::product([value.to_u256().into()])
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Identity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        spacetimedb_sats::ser::serde::serialize_to(&self.to_byte_array(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Identity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(Identity::from_byte_array(arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_sats::GroundSpacetimeType as _;

    #[test]
    fn identity_is_special() {
        assert!(Identity::get_type().is_special());
    }
}
