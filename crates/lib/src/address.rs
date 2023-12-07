use anyhow::Context as _;
use hex::FromHex as _;
use sats::{impl_deserialize, impl_serialize, impl_st, AlgebraicType};
use spacetimedb_bindings_macro::{Deserialize, Serialize};
use spacetimedb_metrics::impl_prometheusvalue_string;
use spacetimedb_metrics::typed_prometheus::AsPrometheusLabel;
use std::{fmt, net::Ipv6Addr};

use crate::sats;
use spacetimedb_sats::hex::HexString;

/// This is the address for a SpacetimeDB database or client connection.
///
/// It is a unique identifier for a particular database and once set for a database,
/// does not change.
///
// TODO: Evaluate other possible names: `DatabaseAddress`, `SPAddress`
// TODO: Evaluate replacing this with a literal Ipv6Address
//       which is assigned permanently to a database.
//       This is likely
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Address {
    __address_bytes: [u8; 16],
}

impl_st!([] Address, _ts => AlgebraicType::product([("__address_bytes", AlgebraicType::bytes())]));
impl_prometheusvalue_string!(Address);

impl Default for Address {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.to_hex())
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Address").field(&format_args!("{self}")).finish()
    }
}

impl Address {
    pub const ZERO: Self = Self {
        __address_bytes: [0; 16],
    };

    pub fn from_arr(arr: &[u8; 16]) -> Self {
        Self { __address_bytes: *arr }
    }

    pub const fn zero() -> Self {
        Self::ZERO
    }

    pub fn from_u128(u: u128) -> Self {
        Self::from_arr(&u.to_be_bytes())
    }

    pub fn from_hex(hex: &str) -> Result<Self, anyhow::Error> {
        <[u8; 16]>::from_hex(hex)
            .context("Addresses must be 32 hex characters (16 bytes) in length.")
            .map(|arr| Self::from_arr(&arr))
    }

    pub fn to_hex(self) -> HexString<16> {
        spacetimedb_sats::hex::encode(self.as_slice())
    }

    pub fn abbreviate(&self) -> [u8; 8] {
        self.as_slice()[..8].try_into().unwrap()
    }

    pub fn to_abbreviated_hex(self) -> HexString<8> {
        spacetimedb_sats::hex::encode(&self.abbreviate())
    }

    pub fn from_slice(slice: impl AsRef<[u8]>) -> Self {
        let slice = slice.as_ref();
        let mut dst = [0u8; 16];
        dst.copy_from_slice(slice);
        Self::from_arr(&dst)
    }

    pub fn as_slice(&self) -> &[u8; 16] {
        &self.__address_bytes
    }

    pub fn to_ipv6(self) -> Ipv6Addr {
        Ipv6Addr::from(self.__address_bytes)
    }

    #[allow(dead_code)]
    pub fn to_ipv6_string(self) -> String {
        self.to_ipv6().to_string()
    }

    #[doc(hidden)]
    pub const __DUMMY: Self = Self::ZERO;

    pub fn to_u128(&self) -> u128 {
        u128::from_be_bytes(self.__address_bytes)
    }
}

impl From<u128> for Address {
    fn from(value: u128) -> Self {
        Self::from_u128(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AddressForUrl(u128);

impl From<Address> for AddressForUrl {
    fn from(addr: Address) -> Self {
        AddressForUrl(u128::from_be_bytes(addr.__address_bytes))
    }
}

impl From<AddressForUrl> for Address {
    fn from(addr: AddressForUrl) -> Self {
        Address::from_u128(addr.0)
    }
}

impl_serialize!([] AddressForUrl, (self, ser) => self.0.to_be_bytes().serialize(ser));
impl_deserialize!([] AddressForUrl, de => <[u8; 16]>::deserialize(de).map(|v| Self(u128::from_be_bytes(v))));
impl_st!([] AddressForUrl, _ts => AlgebraicType::bytes());

#[cfg(feature = "serde")]
impl serde::Serialize for AddressForUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        spacetimedb_sats::ser::serde::serialize_to(&Address::from(*self).as_slice(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AddressForUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(Address::from_arr(&arr).into())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        spacetimedb_sats::ser::serde::serialize_to(&self.as_slice(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(Address::from_arr(&arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bsatn_roundtrip() {
        let addr = Address::from_u128(rand::random());
        let ser = sats::bsatn::to_vec(&addr).unwrap();
        let de = sats::bsatn::from_slice(&ser).unwrap();
        assert_eq!(addr, de);
    }

    #[cfg(feature = "serde")]
    mod serde {
        use super::*;

        #[test]
        fn test_serde_roundtrip() {
            let addr = Address::from_u128(rand::random());
            let to_url = AddressForUrl::from(addr);
            let ser = serde_json::to_vec(&to_url).unwrap();
            let de = serde_json::from_slice::<AddressForUrl>(&ser).unwrap();
            let from_url = Address::from(de);
            assert_eq!(addr, from_url);
        }
    }
}
