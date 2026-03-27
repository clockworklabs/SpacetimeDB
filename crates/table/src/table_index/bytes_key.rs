use super::{DecodeResult, RowRef};
use crate::indexes::RowPointer;
use core::mem;
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::bsatn::{DecodeError, Deserializer, Serializer};
use spacetimedb_sats::de::{DeserializeSeed, Error as _};
use spacetimedb_sats::{u256, AlgebraicType, AlgebraicValue, ProductTypeElement, Serialize as _, WithTypespace};

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
/// A key is then padded with zeroes to the nearest `N`.
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

/// Returns the number of bytes required at most to store a key at `ty`
/// when serialized in BSATN format.
///
/// If keys at `ty` are incompatible with fixed byte keys,
/// e.g., because they are of unbounded length,
/// then `None` is returned.
pub(super) fn required_bytes_key_size(ty: &AlgebraicType) -> Option<usize> {
    use AlgebraicType::*;

    match ty {
        Ref(_) => unreachable!("should not have references at this point"),

        // Variable length types are incompatible with fixed byte keys.
        String | Array(_) => None,

        // For sum, we report the greatest possible fixed size.
        // A key may be of variable size, a long as it fits within an upper bound.
        Sum(ty) => {
            let mut max_size = 0;
            for var in &ty.variants {
                let variant_size = required_bytes_key_size(&var.algebraic_type)?;
                max_size = max_size.max(variant_size);
            }
            // The sum tag is represented as a u8 in BSATN,
            // so add a byte for the tag.
            Some(1 + max_size)
        }

        // For a product, we report the sum of the fixed sizes of the elements.
        Product(ty) => {
            let mut total_size = 0;
            for elem in &ty.elements {
                total_size += required_bytes_key_size(&elem.algebraic_type)?;
            }
            Some(total_size)
        }

        // Primitives:
        Bool | U8 | I8 => Some(mem::size_of::<u8>()),
        U16 | I16 => Some(mem::size_of::<u16>()),
        U32 | I32 | F32 => Some(mem::size_of::<u32>()),
        U64 | I64 | F64 => Some(mem::size_of::<u64>()),
        U128 | I128 => Some(mem::size_of::<u128>()),
        U256 | I256 => Some(mem::size_of::<u256>()),
    }
}

impl<const N: usize> BytesKey<N> {
    /// Decodes `self` as an [`AlgebraicValue`] at `key_type`.
    ///
    /// An incorrect `key_type`,
    /// i.e., one other than what was used when the index was created,
    /// may lead to a panic, but this is not guaranteed.
    /// The method could also silently succeed
    /// if the passed `key_type` incidentally happens to be compatible the stored bytes in `self`.
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
        //
        // The BSATN can originate from untrusted sources, e.g., from module code.
        // This also means that a `BytesKey` can be trusted to hold valid BSATN
        // for the key type, which we can rely on in e.g., `decode_algebraic_value`,
        // which isn't used in a context where it would be appropriate to fail.
        //
        // Another reason to validate is that we wish for `BytesKey` to be strictly
        // an optimization and not allow things that would be rejected by the non-optimized code.
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
        // Validate the BSATN. See `Self::from_bsatn_prefix_and_endpoint` for more details.
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
    ///
    /// SAFETY: Any `col` in `cols` is in-bounds of `row_ref`'s layout.
    pub(super) unsafe fn from_row_ref(cols: &ColList, row_ref: RowRef<'_>) -> Self {
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

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;
    use spacetimedb_sats::bsatn::to_len;
    use spacetimedb_sats::proptest::generate_typed_row;

    const N: usize = 4096;

    proptest! {
        #[test]
        fn test_bytes_key_round_trip((ty, av) in generate_typed_row()) {
            let len = to_len(&av).unwrap();
            prop_assume!(len <= N);

            let ty = AlgebraicType::Product(ty);
            let av = AlgebraicValue::Product(av);
            let key = BytesKey::<N>::from_algebraic_value(&av);
            let decoded_av = key.decode_algebraic_value(&ty);
            assert_eq!(av, decoded_av);
        }

        /*
        // This test turned out not to hold for integers larger than u8,
        // as BSATN stores them little-endian,
        // but `Ord for AlgebraicValue` compares them as big-endian.
        // It's included here for posterity and in case we'd like to
        // massage the BSATN before storing it in the `BytesKey`
        // to make it order-preserving.

        use proptest::array::uniform;
        use spacetimedb_sats::proptest::{gen_with, generate_product_value, generate_row_type, SIZE};

        #[test]
        fn order_in_bsatn_is_preserved((ty, [r1, r2]) in gen_with(generate_row_type(0..=SIZE), |ty| uniform(generate_product_value(ty)))) {
            let ty: AlgebraicType = ty.into();
            let r1: AlgebraicValue = r1.into();
            let r2: AlgebraicValue = r2.into();

            let Some(required) = required_bytes_key_size(&ty, true) else {
                //dbg!(&ty);
                return Err(TestCaseError::reject("type is incompatible with fixed byte keys in range indices"));
            };
            prop_assume!(required <= N);

            let k1 = BytesKey::<N>::from_algebraic_value(&r1);
            let k2 = BytesKey::<N>::from_algebraic_value(&r2);
            let ord_k = k1.cmp(&k2);
            let ord_r = r1.cmp(&r2);
            prop_assert_eq!(ord_k, ord_r);
        }
        */
    }
}
