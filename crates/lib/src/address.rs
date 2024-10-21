use anyhow::Context as _;
use core::{fmt, net::Ipv6Addr};
use spacetimedb_bindings_macro::{Deserialize, Serialize};
use spacetimedb_lib::from_hex_pad;
use spacetimedb_sats::hex::HexString;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, AlgebraicType, AlgebraicValue};

/// This is the address for a SpacetimeDB database or client connection.
///
/// TODO: This is wrong; the address can change, but the Identity cannot.
/// It is a unique identifier for a particular database and once set for a database,
/// does not change.
///
/// This is a special type.
///
// TODO: Evaluate other possible names: `DatabaseAddress`, `SPAddress`
// TODO: Evaluate replacing this with a literal Ipv6Address
//       which is assigned permanently to a database.
//       This is likely
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Address {
    __address__: u128,
}

impl_st!([] Address, AlgebraicType::address());

#[cfg(feature = "metrics_impls")]
impl spacetimedb_metrics::typed_prometheus::AsPrometheusLabel for Address {
    fn as_prometheus_str(&self) -> impl AsRef<str> + '_ {
        self.to_hex()
    }
}

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
    pub const ZERO: Self = Self::from_u128(0);

    pub const fn from_u128(__address__: u128) -> Self {
        Self { __address__ }
    }

    pub const fn to_u128(&self) -> u128 {
        self.__address__
    }

    pub const fn from_byte_array(arr: [u8; 16]) -> Self {
        Self::from_u128(u128::from_le_bytes(arr))
    }

    pub const fn as_byte_array(&self) -> [u8; 16] {
        self.__address__.to_le_bytes()
    }

    pub fn from_hex(hex: &str) -> Result<Self, anyhow::Error> {
        from_hex_pad::<[u8; 16], _>(hex)
            .context("Addresses must be 32 hex characters (16 bytes) in length.")
            .map(Self::from_byte_array)
    }

    pub fn to_hex(self) -> HexString<16> {
        spacetimedb_sats::hex::encode(&self.as_byte_array())
    }

    pub fn abbreviate(&self) -> [u8; 8] {
        self.as_byte_array()[..8].try_into().unwrap()
    }

    pub fn to_abbreviated_hex(self) -> HexString<8> {
        spacetimedb_sats::hex::encode(&self.abbreviate())
    }

    pub fn from_slice(slice: impl AsRef<[u8]>) -> Self {
        let slice = slice.as_ref();
        let mut dst = [0u8; 16];
        dst.copy_from_slice(slice);
        Self::from_byte_array(dst)
    }

    pub fn to_ipv6(self) -> Ipv6Addr {
        Ipv6Addr::from(self.__address__)
    }

    #[allow(dead_code)]
    pub fn to_ipv6_string(self) -> String {
        self.to_ipv6().to_string()
    }

    #[doc(hidden)]
    pub const __DUMMY: Self = Self::ZERO;

    pub fn none_if_zero(self) -> Option<Self> {
        (self != Self::ZERO).then_some(self)
    }
}

impl From<u128> for Address {
    fn from(value: u128) -> Self {
        Self::from_u128(value)
    }
}

impl From<Address> for AlgebraicValue {
    fn from(value: Address) -> Self {
        AlgebraicValue::product([value.to_u128().into()])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AddressForUrl(u128);

impl From<Address> for AddressForUrl {
    fn from(addr: Address) -> Self {
        AddressForUrl(addr.to_u128())
    }
}

impl From<AddressForUrl> for Address {
    fn from(addr: AddressForUrl) -> Self {
        Address::from_u128(addr.0)
    }
}

impl_serialize!([] AddressForUrl, (self, ser) => self.0.serialize(ser));
impl_deserialize!([] AddressForUrl, de => u128::deserialize(de).map(Self));
impl_st!([] AddressForUrl, AlgebraicType::U128);

#[cfg(feature = "serde")]
impl serde::Serialize for AddressForUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        spacetimedb_sats::ser::serde::serialize_to(&Address::from(*self).as_byte_array(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AddressForUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(Address::from_byte_array(arr).into())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        spacetimedb_sats::ser::serde::serialize_to(&self.as_byte_array(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(Address::from_byte_array(arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use spacetimedb_sats::bsatn;
    use spacetimedb_sats::GroundSpacetimeType as _;

    proptest! {
        #[test]
        fn test_bsatn_roundtrip(val: u128) {
            let addr = Address::from_u128(val);
            let ser = bsatn::to_vec(&addr).unwrap();
            let de = bsatn::from_slice(&ser).unwrap();
            assert_eq!(addr, de);
        }
    }

    #[test]
    fn address_is_special() {
        assert!(Address::get_type().is_special());
    }

    #[cfg(feature = "serde")]
    mod serde {
        use super::*;
        use crate::sats::{algebraic_value::de::ValueDeserializer, de::Deserialize, Typespace};
        use crate::ser::serde::SerializeWrapper;
        use crate::WithTypespace;

        proptest! {
            /// Tests the round-trip used when using the `spacetime subscribe`
            /// CLI command.
            /// Somewhat confusingly, this is distinct from the ser-de path
            /// in `test_serde_roundtrip`.
            #[test]
            fn test_wrapper_roundtrip(val: u128) {
                let addr = Address::from_u128(val);
                let wrapped = SerializeWrapper::new(&addr);

                let ser = serde_json::to_string(&wrapped).unwrap();
                let empty = Typespace::default();
                let address_ty = Address::get_type();
                let address_ty = WithTypespace::new(&empty, &address_ty);
                let row = serde_json::from_str::<serde_json::Value>(&ser[..])?;
                let de = ::serde::de::DeserializeSeed::deserialize(
                    crate::de::serde::SeedWrapper(
                        address_ty
                    ),
                    row)?;
                let de = Address::deserialize(ValueDeserializer::new(de)).unwrap();
                prop_assert_eq!(addr, de);
            }
        }

        proptest! {
            #[test]
            fn test_serde_roundtrip(val: u128) {
                let addr = Address::from_u128(val);
                let to_url = AddressForUrl::from(addr);
                let ser = serde_json::to_vec(&to_url).unwrap();
                let de = serde_json::from_slice::<AddressForUrl>(&ser).unwrap();
                let from_url = Address::from(de);
                prop_assert_eq!(addr, from_url);
            }
        }
    }
}
