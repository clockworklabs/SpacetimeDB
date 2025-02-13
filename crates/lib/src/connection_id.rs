use anyhow::Context as _;
use core::{fmt, net::Ipv6Addr};
use spacetimedb_bindings_macro::{Deserialize, Serialize};
use spacetimedb_lib::from_hex_pad;
use spacetimedb_sats::hex::HexString;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, AlgebraicType, AlgebraicValue};

/// A unique identifier for a client connection to a SpacetimeDB database.
///
/// This is a special type.
///
/// A `ConnectionId` is a 128-bit unsigned integer. This can be serialized in various ways.
/// - In JSON, an `ConnectionId` is represented as a BARE DECIMAL number.
///   This requires some care when deserializing; see
///   <https://stackoverflow.com/questions/69644298/how-to-make-json-parse-to-treat-all-the-numbers-as-bigint>
/// - In BSATN, a `ConnectionId` is represented as a LITTLE-ENDIAN number 16 bytes long.
/// - In memory, a `ConnectionId` is stored as a 128-bit number with the endianness of the host system.
//
// If you are manually converting a hexadecimal string to a byte array like so:
// ```ignore
// "0xb0b1b2..."
// ->
// [0xb0, 0xb1, 0xb2, ...]
// ```
// Make sure you call `ConnectionId::from_be_byte_array` and NOT `ConnectionId::from_le_byte_array`.
// The standard way of writing hexadecimal numbers follows a big-endian convention, if you
// index the characters in written text in increasing order from left to right.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConnectionId {
    __connection_id__: u128,
}

impl_st!([] ConnectionId, AlgebraicType::connection_id());

#[cfg(feature = "metrics_impls")]
impl spacetimedb_metrics::typed_prometheus::AsPrometheusLabel for ConnectionId {
    fn as_prometheus_str(&self) -> impl AsRef<str> + '_ {
        self.to_hex()
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.to_hex())
    }
}

impl fmt::Debug for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ConnectionId").field(&format_args!("{self}")).finish()
    }
}

impl ConnectionId {
    pub const ZERO: Self = Self::from_u128(0);

    pub const fn from_u128(__connection_id__: u128) -> Self {
        Self { __connection_id__ }
    }

    pub const fn to_u128(&self) -> u128 {
        self.__connection_id__
    }

    /// Create an `ConnectionId` from a little-endian byte array.
    ///
    /// If you are parsing an `ConnectionId` from a string,
    /// you probably want [`Self::from_be_byte_array`] instead.
    /// But if you need to convert a hexadecimal string to a `ConnectionId`,
    /// just use [`Self::from_hex`].
    pub const fn from_le_byte_array(arr: [u8; 16]) -> Self {
        Self::from_u128(u128::from_le_bytes(arr))
    }

    /// Create an `ConnectionId` from a big-endian byte array.
    ///
    /// This method is the correct choice
    /// if you have converted the bytes of a hexadecimal-formatted `ConnectionId`
    /// to a byte array in the following way:
    ///
    /// ```ignore
    /// "0xb0b1b2..."
    /// ->
    /// [0xb0, 0xb1, 0xb2, ...]
    /// ```
    ///
    /// But if you need to convert a hexadecimal string to a `ConnectionId`,
    /// just use [`Self::from_hex`].
    pub const fn from_be_byte_array(arr: [u8; 16]) -> Self {
        Self::from_u128(u128::from_be_bytes(arr))
    }

    /// Convert a `ConnectionId` to a little-endian byte array.
    pub const fn as_le_byte_array(&self) -> [u8; 16] {
        self.__connection_id__.to_le_bytes()
    }

    /// Convert a `ConnectionId` to a big-endian byte array.
    ///
    /// This is a format suitable for printing as a hexadecimal string.
    /// But if you need to convert a `ConnectionId` to a hexadecimal string,
    /// just use [`Self::to_hex`].
    pub const fn as_be_byte_array(&self) -> [u8; 16] {
        self.__connection_id__.to_be_bytes()
    }

    /// Parse a hexadecimal string into a `ConnectionId`.
    pub fn from_hex(hex: &str) -> Result<Self, anyhow::Error> {
        from_hex_pad::<[u8; 16], _>(hex)
            .context("ConnectionIds must be 32 hex characters (16 bytes) in length.")
            .map(Self::from_be_byte_array)
    }

    /// Convert this `ConnectionId` to a hexadecimal string.
    pub fn to_hex(self) -> HexString<16> {
        spacetimedb_sats::hex::encode(&self.as_be_byte_array())
    }

    /// Extract the first 8 bytes of this `ConnectionId` as if it was stored in big-endian
    /// format. (That is, the most significant bytes.)
    pub fn abbreviate(&self) -> [u8; 8] {
        self.as_be_byte_array()[..8].try_into().unwrap()
    }

    /// Extract the first 16 characters of this `ConnectionId`'s hexadecimal representation.
    pub fn to_abbreviated_hex(self) -> HexString<8> {
        spacetimedb_sats::hex::encode(&self.abbreviate())
    }

    /// Create an `ConnectionId` from a slice, assumed to be in big-endian format.
    pub fn from_be_slice(slice: impl AsRef<[u8]>) -> Self {
        let slice = slice.as_ref();
        let mut dst = [0u8; 16];
        dst.copy_from_slice(slice);
        Self::from_be_byte_array(dst)
    }

    pub fn to_ipv6(self) -> Ipv6Addr {
        Ipv6Addr::from(self.__connection_id__)
    }

    #[allow(dead_code)]
    pub fn to_ipv6_string(self) -> String {
        self.to_ipv6().to_string()
    }

    pub fn none_if_zero(self) -> Option<Self> {
        (self != Self::ZERO).then_some(self)
    }
}

impl From<u128> for ConnectionId {
    fn from(value: u128) -> Self {
        Self::from_u128(value)
    }
}

impl From<ConnectionId> for AlgebraicValue {
    fn from(value: ConnectionId) -> Self {
        AlgebraicValue::product([value.to_u128().into()])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionIdForUrl(u128);

impl From<ConnectionId> for ConnectionIdForUrl {
    fn from(addr: ConnectionId) -> Self {
        ConnectionIdForUrl(addr.to_u128())
    }
}

impl From<ConnectionIdForUrl> for ConnectionId {
    fn from(addr: ConnectionIdForUrl) -> Self {
        ConnectionId::from_u128(addr.0)
    }
}

impl_serialize!([] ConnectionIdForUrl, (self, ser) => self.0.serialize(ser));
impl_deserialize!([] ConnectionIdForUrl, de => u128::deserialize(de).map(Self));
impl_st!([] ConnectionIdForUrl, AlgebraicType::U128);

#[cfg(feature = "serde")]
impl serde::Serialize for ConnectionIdForUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        spacetimedb_sats::ser::serde::serialize_to(&ConnectionId::from(*self).as_be_byte_array(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ConnectionIdForUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(ConnectionId::from_be_byte_array(arr).into())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ConnectionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        spacetimedb_sats::ser::serde::serialize_to(&self.as_be_byte_array(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ConnectionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(ConnectionId::from_be_byte_array(arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use spacetimedb_sats::bsatn;
    use spacetimedb_sats::ser::serde::SerializeWrapper;
    use spacetimedb_sats::GroundSpacetimeType as _;

    #[test]
    fn connection_id_json_serialization_big_endian() {
        let conn_id = ConnectionId::from_be_byte_array([0xff, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);

        let hex = conn_id.to_hex();
        assert!(
            hex.as_str().starts_with("ff01"),
            "expected {hex:?} to start with \"ff01\""
        );

        let json1 = serde_json::to_string(&conn_id).unwrap();
        let json2 = serde_json::to_string(&ConnectionIdForUrl::from(conn_id)).unwrap();

        assert!(
            json1.contains(hex.as_str()),
            "expected {json1} to contain {hex} but it didn't"
        );
        assert!(
            json2.contains(hex.as_str()),
            "expected {json2} to contain {hex} but it didn't"
        );

        // Serde made the slightly odd choice to serialize u128 as decimals in JSON.
        // So we have an incompatibility between our formats here :/
        // The implementation of serialization for `sats` types via `SerializeWrapper` just calls
        // the `serde` implementation to serialize primitives, so we can't fix this
        // unless we make a custom implementation of `Serialize` and `Deserialize` for `ConnectionId`.
        let decimal = conn_id.to_u128().to_string();
        let json3 = serde_json::to_string(SerializeWrapper::from_ref(&conn_id)).unwrap();
        assert!(
            json3.contains(decimal.as_str()),
            "expected {json3} to contain {decimal} but it didn't"
        );
    }

    proptest! {
        #[test]
        fn test_bsatn_roundtrip(val: u128) {
            let conn_id = ConnectionId::from_u128(val);
            let ser = bsatn::to_vec(&conn_id).unwrap();
            let de = bsatn::from_slice(&ser).unwrap();
            assert_eq!(conn_id, de);
        }

        #[test]
        fn connection_id_conversions(a: u128) {
            let v = ConnectionId::from_u128(a);

            prop_assert_eq!(ConnectionId::from_le_byte_array(v.as_le_byte_array()), v);
            prop_assert_eq!(ConnectionId::from_be_byte_array(v.as_be_byte_array()), v);
            prop_assert_eq!(ConnectionId::from_hex(v.to_hex().as_str()).unwrap(), v);
        }
    }

    #[test]
    fn connection_id_is_special() {
        assert!(ConnectionId::get_type().is_special());
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
                let conn_id = ConnectionId::from_u128(val);
                let wrapped = SerializeWrapper::new(&conn_id);

                let ser = serde_json::to_string(&wrapped).unwrap();
                let empty = Typespace::default();
                let conn_id_ty = ConnectionId::get_type();
                let conn_id_ty = WithTypespace::new(&empty, &conn_id_ty);
                let row = serde_json::from_str::<serde_json::Value>(&ser[..])?;
                let de = ::serde::de::DeserializeSeed::deserialize(
                    crate::de::serde::SeedWrapper(
                        conn_id_ty
                    ),
                    row)?;
                let de = ConnectionId::deserialize(ValueDeserializer::new(de)).unwrap();
                prop_assert_eq!(conn_id, de);
            }
        }

        proptest! {
            #[test]
            fn test_serde_roundtrip(val: u128) {
                let conn_id = ConnectionId::from_u128(val);
                let to_url = ConnectionIdForUrl::from(conn_id);
                let ser = serde_json::to_vec(&to_url).unwrap();
                let de = serde_json::from_slice::<ConnectionIdForUrl>(&ser).unwrap();
                let from_url = ConnectionId::from(de);
                prop_assert_eq!(conn_id, from_url);
            }
        }
    }
}
