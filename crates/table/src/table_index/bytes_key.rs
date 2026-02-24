use super::{DecodeResult, RowRef};
use crate::indexes::RowPointer;
use core::mem;
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::bsatn::{DecodeError, Deserializer, Serializer};
use spacetimedb_sats::de::{DeserializeSeed, Error as _};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductTypeElement, Serialize as _, WithTypespace};

/// A key for an all-primitive multi-column index
/// serialized to a byte array.
///
/// The key can store up to `N` bytes
/// where `N` is determined by the summed size of each column in the index
/// when serialized in BSATN format,
/// which is the same as little-endian encoding of the keys for primitive types.
///
/// As we cannot have too many different `N`s,
/// we have a few `N`s, where each is a power of 2.
/// A key is then padded to the nearest `N`.
/// For example, a key `(x: u8, y: u16, z: u32)` for a 3-column index
/// would have `N = 1 + 2 + 4 = 7` but would be padded to `N = 8`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub(super) struct BytesKey<const N: usize>([u8; N]);

impl<const N: usize> MemoryUsage for BytesKey<N> {}

/// A difference between btree indices and hash indices
/// is that the former btree indices store keys and values separately,
/// i.e., as `([K], [RowPointer])`
/// whereas hash indices store them together,
/// i.e., as `([K, RowPointer])`.
///
/// For hash indices, it's therefore profitable to ensure
/// that the key and the value together fit into an `N` that is a power of 2.
/// An `N` that is a power of 2 is well aligned around cache line sizes.
pub(super) const fn size_sub_row_pointer(n: usize) -> usize {
    n - mem::size_of::<RowPointer>()
}

impl<const N: usize> BytesKey<N> {
    /// Decodes `self` as an [`AlgebraicValue`] at `key_type`.
    ///
    /// Panics if the wrong `key_type` is provided, but that should never happen.
    pub(super) fn decode_algebraic_value(&self, key_type: &AlgebraicType) -> AlgebraicValue {
        AlgebraicValue::decode(key_type, &mut self.0.as_slice())
            .expect("A `BytesKey` should by construction always deserialize to the right `key_type`")
    }

    /// Ensure bytes of length `got` fit in `N` or return an error.
    fn ensure_key_fits(got: usize) -> DecodeResult<()> {
        if got > N {
            return Err(DecodeError::custom(format_args!(
                "key provided is too long, expected at most {N}, but got {got}"
            )));
        }
        Ok(())
    }

    /// Decodes `prefix` and `endpoint` in BSATN to a [`BytesKey<N>`]
    /// by copying over both if they fit into the key.
    pub(super) fn from_bsatn_prefix_and_endpoint(
        prefix: &[u8],
        prefix_types: &[ProductTypeElement],
        endpoint: &[u8],
        range_type: &AlgebraicType,
    ) -> DecodeResult<Self> {
        // Validate the BSATN.
        WithTypespace::empty(prefix_types).validate(Deserializer::new(&mut { prefix }))?;
        WithTypespace::empty(range_type).validate(Deserializer::new(&mut { endpoint }))?;
        // Check that the `prefix` and the `endpoint` together fit into the key.
        let prefix_len = prefix.len();
        let endpoint_len = endpoint.len();
        Self::ensure_key_fits(prefix_len + endpoint_len)?;
        // Copy the `prefix` and the `endpoint` over.
        let mut arr = [0; N];
        arr[..prefix_len].copy_from_slice(prefix);
        arr[prefix_len..prefix_len + endpoint_len].copy_from_slice(endpoint);
        Ok(Self(arr))
    }

    /// Decodes `bytes` in BSATN to a [`BytesKey<N>`]
    /// by copying over the bytes if they fit into the key.
    pub(super) fn from_bsatn(ty: &AlgebraicType, bytes: &[u8]) -> DecodeResult<Self> {
        // Validate the BSATN.
        WithTypespace::empty(ty).validate(Deserializer::new(&mut { bytes }))?;
        // Check that the `bytes` fit into the key.
        let got = bytes.len();
        Self::ensure_key_fits(got)?;
        // Copy the bytes over.
        let mut arr = [0; N];
        arr[..got].copy_from_slice(bytes);
        Ok(Self(arr))
    }

    /// Serializes the columns `cols` in `row_ref` to a [`BytesKey<N>`].
    ///
    /// It's assumed that `row_ref` projected to `cols`
    /// will fit into `N` bytes when serialized into BSATN.
    /// The method panics otherwise.
    pub(super) fn from_row_ref(cols: &ColList, row_ref: RowRef<'_>) -> Self {
        let mut arr = [0; N];
        let mut sink = arr.as_mut_slice();
        let ser = Serializer::new(&mut sink);
        unsafe { row_ref.serialize_columns_unchecked(cols, ser) }
            .expect("should've serialized a `row_ref` to BSATN successfully");
        Self(arr)
    }

    /// Serializes `av` to a [`BytesKey<N>`].
    ///
    /// It's assumed that `av`
    /// will fit into `N` bytes when serialized into BSATN.
    /// The method panics otherwise.
    pub(super) fn from_algebraic_value(av: &AlgebraicValue) -> Self {
        let mut arr = [0; N];
        let mut sink = arr.as_mut_slice();
        let ser = Serializer::new(&mut sink);
        av.serialize_into_bsatn(ser)
            .expect("should've serialized an `AlgebraicValue` to BSATN successfully");
        Self(arr)
    }
}
