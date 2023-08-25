use std::{fmt::Display, net::Ipv6Addr};

use anyhow::Context as _;
use hex::FromHex as _;
use sats::{impl_deserialize, impl_serialize, impl_st};

use crate::sats;

/// This is the address for a SpacetimeDB database. It is a unique identifier
/// for a particular database and once set for a database, does not change.
///
/// TODO: Evaluate other possible names: `DatabaseAddress`, `SPAddress`
/// TODO: Evaluate replacing this with a literal Ipv6Address which is assigned
/// permanently to a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address(u128);

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Address {
    const ABBREVIATION_LEN: usize = 16;

    pub const ZERO: Self = Self(0);

    pub fn from_arr(arr: &[u8; 16]) -> Self {
        Self(u128::from_be_bytes(*arr))
    }

    pub fn from_hex(hex: &str) -> Result<Self, anyhow::Error> {
        <[u8; 16]>::from_hex(hex)
            .context("Addresses must be 32 hex characters (16 bytes) in length.")
            .map(u128::from_be_bytes)
            .map(Self)
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.as_slice())
    }

    pub fn to_abbreviated_hex(self) -> String {
        self.to_hex()[0..Self::ABBREVIATION_LEN].to_owned()
    }

    pub fn from_slice(slice: impl AsRef<[u8]>) -> Self {
        let slice = slice.as_ref();
        let mut dst = [0u8; 16];
        dst.copy_from_slice(slice);
        Self(u128::from_be_bytes(dst))
    }

    pub fn as_slice(&self) -> [u8; 16] {
        self.0.to_be_bytes()
    }

    pub fn to_ipv6(self) -> Ipv6Addr {
        Ipv6Addr::from(self.0)
    }

    #[allow(dead_code)]
    pub fn to_ipv6_string(self) -> String {
        self.to_ipv6().to_string()
    }

    pub fn to_u128(&self) -> u128 {
        self.0
    }
}

impl From<u128> for Address {
    fn from(value: u128) -> Self {
        Self(value)
    }
}

impl_serialize!([] Address, (self, ser) => self.0.to_be_bytes().serialize(ser));
impl_deserialize!([] Address, de => <[u8; 16]>::deserialize(de).map(|v| Self(u128::from_be_bytes(v))));

#[cfg(feature = "serde")]
impl serde::Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_hex().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Address::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl_st!([] Address, _ts => sats::AlgebraicType::bytes());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bsatn_roundtrip() {
        let addr = Address(rand::random());
        let ser = sats::bsatn::to_vec(&addr).unwrap();
        let de = sats::bsatn::from_slice(&ser).unwrap();
        assert_eq!(addr, de);
    }

    #[cfg(feature = "serde")]
    mod serde {
        use super::*;

        #[test]
        fn test_serde_roundtrip() {
            let addr = Address(rand::random());
            let ser = serde_json::to_vec(&addr).unwrap();
            let de = serde_json::from_slice(&ser).unwrap();
            assert_eq!(addr, de);
        }
    }
}
