use std::net::Ipv6Addr;

use anyhow::Context as _;
use hex::FromHex as _;
use sats::{impl_st, impl_serialize};

use crate::sats::{self, de};

/// This is the address for a SpacetimeDB database. It is a unique identifier
/// for a particular database and once set for a database, does not change.
///
/// TODO: Evaluate other possible names: `DatabaseAddress`, `SPAddress`
/// TODO: Evaluate replacing this with a literal Ipv6Address which is assigned
/// permanently to a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Address(u128);

impl Address {
    const ABBREVIATION_LEN: usize = 16;

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
}

impl_serialize!([] Address, (self, ser) =>self.0.to_be_bytes().serialize(ser));

impl<'de> de::Deserialize<'de> for Address {
    fn deserialize<D: spacetimedb_sats::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <[u8; 16]>::deserialize(deserializer).map(|v| Self(u128::from_be_bytes(v)))
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

impl_st!([] Address, _ts => sats::AlgebraicType::U128);
