use crate::from_hex_pad;
use blake3;
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

/// An `Identity` for something interacting with the database.
///
/// An `Identity` is a 256-bit unsigned integer. These are encoded in various ways.
/// - In JSON, an `Identity` is represented as a hexadecimal number wrapped in a string, `"0x[64 hex characters]"`.
/// - In BSATN, an `Identity` is represented as a LITTLE-ENDIAN number 32 bytes long.
/// - In memory, an `Identity` is stored as a 256-bit number with the endianness of the host system.
///
/// If you are manually converting a hexadecimal string to a byte array like so:
/// ```ignore
/// "0xb0b1b2..."
/// ->
/// [0xb0, 0xb1, 0xb2, ...]
/// ```
/// Make sure you call `Identity::from_be_byte_array` and NOT `Identity::from_byte_array`.
/// The standard way of writing hexadecimal numbers follows a big-endian convention, if you
/// index the characters in written text in increasing order from left to right.
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

impl Identity {
    pub const ZERO: Self = Self::from_u256(u256::ZERO);

    /// Create an `Identity` from a LITTLE-ENDIAN byte array.
    ///
    /// If you are parsing an `Identity` from a string, you probably want `from_be_byte_array` instead.
    pub const fn from_byte_array(bytes: [u8; 32]) -> Self {
        // SAFETY: The transmute is an implementation of `u256::from_le_bytes`,
        // but works in a const context.
        Self::from_u256(u256::from_le(unsafe { mem::transmute(bytes) }))
    }

    /// Create an `Identity` from a BIG-ENDIAN byte array.
    ///
    /// This method is the correct choice if you have converted the bytes of a hexadecimal-formatted `Identity`
    /// to a byte array in the following way:
    /// ```ignore
    /// "0xb0b1b2..."
    /// ->
    /// [0xb0, 0xb1, 0xb2, ...]
    /// ```
    /// If you have created the byte array from a hexadecimal string, use `from_be_byte_array` instead!
    pub const fn from_be_byte_array(bytes: [u8; 32]) -> Self {
        // SAFETY: The transmute is an implementation of `u256::from_le_bytes`,
        // but works in a const context.
        Self::from_u256(u256::from_be(unsafe { mem::transmute(bytes) }))
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
    /// The slice is assumed to be in big-endian format.
    pub fn from_slice(slice: &[u8]) -> Self {
        Self::from_be_byte_array(slice.try_into().unwrap())
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

        // We want the leading two bytes of the Identity to be `c200` when formatted.
        // This means that these should be the MOST significant bytes.
        // This corresponds to a BIG-ENDIAN byte order of our buffer above.
        Identity::from_be_byte_array(final_bytes)
    }

    /// Returns this `Identity` as a byte array.
    pub fn to_byte_array(&self) -> [u8; 32] {
        self.__identity__.to_le_bytes()
    }

    /// Convert this `Identity` to a BIG-ENDIAN byte array.
    pub fn to_be_byte_array(&self) -> [u8; 32] {
        self.__identity__.to_be_bytes()
    }

    /// Convert this `Identity` to a hexadecimal string.
    pub fn to_hex(&self) -> HexString<32> {
        spacetimedb_sats::hex::encode(&self.to_be_byte_array())
    }

    /// Extract the first 8 bytes of this `Identity` as if it was stored in BIG-ENDIAN
    /// format. (That is, the most significant bytes.)
    pub fn abbreviate(&self) -> [u8; 8] {
        self.to_be_byte_array()[..8].try_into().unwrap()
    }

    /// Extract the first 16 characters of this `Identity`'s hexadecimal representation.
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
        from_hex_pad(hex).map(Identity::from_be_byte_array)
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
        spacetimedb_sats::ser::serde::serialize_to(&self.to_be_byte_array(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Identity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let arr = spacetimedb_sats::de::serde::deserialize_from(deserializer)?;
        Ok(Identity::from_be_byte_array(arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::string::string_regex;
    use spacetimedb_sats::{de::serde::DeserializeWrapper, ser::serde::SerializeWrapper, GroundSpacetimeType as _};

    #[test]
    fn identity_is_special() {
        assert!(Identity::get_type().is_special());
    }

    #[test]
    fn identity_json_serialization_big_endian() {
        let id = Identity::from_be_byte_array([
            0xff, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
            28, 29, 30, 31,
        ]);

        let hex = id.to_hex();
        let json1 = serde_json::to_string(&id).unwrap();
        let json2 = serde_json::to_string(SerializeWrapper::from_ref(&id)).unwrap();

        assert!(
            json1.contains(hex.as_str()),
            "expected {json1} to contain {hex} but it didn't"
        );
        assert!(
            json2.contains(hex.as_str()),
            "expected {json2} to contain {hex} but it didn't"
        );
    }

    proptest! {
        #[test]
        fn identity_conversions(w0: u128, w1: u128) {
            let v = Identity::from_u256(u256::from_words(w0, w1));

            prop_assert_eq!(Identity::from_byte_array(v.to_byte_array()), v);
            prop_assert_eq!(Identity::from_be_byte_array(v.to_be_byte_array()), v);
            prop_assert_eq!(Identity::from_hex(v.to_hex()).unwrap(), v);

            let de1: Identity = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
            prop_assert_eq!(de1, v);
            let DeserializeWrapper(de2): DeserializeWrapper<Identity> = serde_json::from_str(&serde_json::to_string(SerializeWrapper::from_ref(&v)).unwrap()).unwrap();
            prop_assert_eq!(de2, v);
        }

        #[test]
        fn from_claims_formats_correctly(s1 in string_regex(r".{3,5}").unwrap(), s2 in string_regex(r".{3,5}").unwrap()) {
            let id = Identity::from_claims(&s1, &s2);
            prop_assert!(id.to_hex().starts_with("c200"));
        }
    }
}
