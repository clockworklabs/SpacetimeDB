use super::key_size::KeySize;
use super::{DecodeResult, RowRef};
use crate::indexes::RowPointer;
use core::cmp::Ordering;
use core::hash::{Hash, Hasher};
use core::mem;
use core::ops::Deref;
use spacetimedb_lib::buffer::{CountWriter, TeeWriter};
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::bsatn::{DecodeError, Deserializer, Serializer};
use spacetimedb_sats::de::{DeserializeSeed, Error as _};
use spacetimedb_sats::{i256, u256, AlgebraicType, AlgebraicValue, ProductTypeElement, Serialize as _, WithTypespace};

/// A key for an all-primitive multi-column index
/// serialized to a byte array.
///
/// The key can store up to `N` bytes
/// where `N` is determined by the summed size of each column in the index
/// when serialized in BSATN format,
/// which is the same as little-endian encoding of the keys for primitive types.
///
/// As we cannot have too many different `N`s,
/// we have a few `N`s, where `N = 2^x - 1`.
/// A key is then padded with zeroes to the nearest `N`.
/// For example, a key `(y: u16, z: u32)` for a 2-column index
/// would have `N = 2 + 4 = 6` but would be padded to `N = 7`.
/// The reason for the `-1`, i.e., `N = 7` and not `N = 8`
/// is because `length` takes up one byte.
///
/// The `length` stores the number of actual bytes used by the key.
#[derive(Debug, Eq, Clone, Copy)]
pub(super) struct BytesKey<const N: usize> {
    length: u8,
    bytes: [u8; N],
}

impl<const N: usize> MemoryUsage for BytesKey<N> {}

impl<const N: usize> KeySize for BytesKey<N> {
    type MemoStorage = u64;

    fn key_size_in_bytes(&self) -> usize {
        self.length as usize
    }
}

impl<const N: usize> Deref for BytesKey<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes[0..self.length as usize]
    }
}

impl<const N: usize> PartialEq for BytesKey<N> {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl<const N: usize> PartialOrd for BytesKey<N> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Ord for BytesKey<N> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other.deref())
    }
}

impl<const N: usize> Hash for BytesKey<N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state);
    }
}

/// A difference between btree indices and hash indices
/// is that the former btree indices store keys and values separately,
/// i.e., as `([K], [RowPointer])`
/// whereas hash indices store them together,
/// i.e., as `([K, RowPointer])`.
///
/// For hash indices, it's therefore profitable to ensure
/// that the key and the value together fit into an `N + 1` that is a power of 2.
/// An `N + 1` that is a power of 2 is well aligned around cache line sizes.
pub(super) const fn size_for_hash_bytes_key(n: usize) -> usize {
    size_for_btree_bytes_key(n) - mem::size_of::<RowPointer>()
}

/// A difference between btree indices and hash indices
/// is that the former btree indices store keys and values separately,
/// i.e., as `([K], [RowPointer])`
/// whereas hash indices store them together,
/// i.e., as `([K, RowPointer])`.
///
/// For btree indices, it's therefore sufficient to enure
/// that the key alone fits into an `N + 1` that is a power of 2.
/// An `N + 1` that is a power of 2 is well aligned around cache line sizes.
pub(super) const fn size_for_btree_bytes_key(n: usize) -> usize {
    n - 1
}

/// Returns the number of bytes required at most to store a key at `ty`
/// when serialized in BSATN format.
///
/// If keys at `ty` are incompatible with fixed byte keys,
/// e.g., because they are of unbounded length,
/// or because `is_ranged_idx` and `ty` contains a float,
/// then `None` is returned.
pub(super) fn required_bytes_key_size(ty: &AlgebraicType, is_ranged_idx: bool) -> Option<usize> {
    use AlgebraicType::*;

    match ty {
        Ref(_) => unreachable!("should not have references at this point"),

        // Variable length types are incompatible with fixed byte keys.
        String | Array(_) => None,

        // For sum, we report the greatest possible fixed size.
        // A key may be of variable size, a long as it fits within an upper bound.
        //
        // It's valid to use `RangeCompatBytesKey`-ified sums in range index,
        // i.e., when `is_range_idx`,
        // as `Ord for AlgebraicValue` delegates to `Ord for SumValue`
        // which compares the `tag` first and the payload (`value`) second,
        // The `RangeCompatBytesKey` encoding of sums places the `tag` first and the payload second.
        // When comparing two `[u8]` slices with encoded sums,
        // this produces an ordering that also compares the `tag` first and the payload second.
        Sum(ty) => {
            let mut max_size = 0;
            for var in &ty.variants {
                let variant_size = required_bytes_key_size(&var.algebraic_type, is_ranged_idx)?;
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
                total_size += required_bytes_key_size(&elem.algebraic_type, is_ranged_idx)?;
            }
            Some(total_size)
        }

        // Floats are stored in IEEE 754 format,
        // so their byte representation is not order-preserving.
        F32 | F64 if is_ranged_idx => None,

        // Primitives:
        Bool | U8 | I8 => Some(mem::size_of::<u8>()),
        U16 | I16 => Some(mem::size_of::<u16>()),
        U32 | I32 | F32 => Some(mem::size_of::<u32>()),
        U64 | I64 | F64 => Some(mem::size_of::<u64>()),
        U128 | I128 => Some(mem::size_of::<u128>()),
        U256 | I256 => Some(mem::size_of::<u256>()),
    }
}

/// Validates BSATN `byte` to conform to `seed`.
///
/// The BSATN can originate from untrusted sources, e.g., from module code.
/// This also means that e.g., a `BytesKey` can be trusted to hold valid BSATN
/// for the key type, which we can rely on in e.g., `decode_algebraic_value`,
/// which isn't used in a context where it would be appropriate to fail.
///
/// Another reason to validate is that we wish for `BytesKey` to be strictly
/// an optimization and not allow things that would be rejected by the non-optimized code.
///
/// After validating, we also don't need to validate that `bytes`
/// will fit into e.g., a `BytesKey<N>`
/// since if all parts that are encoded into it are valid according to a key type,
/// then `bytes` cannot be longer than `N`.
fn validate<'a, 'de, S: 'a + ?Sized>(seed: &'a S, mut bytes: &'de [u8]) -> DecodeResult<()>
where
    WithTypespace<'a, S>: DeserializeSeed<'de>,
{
    WithTypespace::empty(seed).validate(Deserializer::new(&mut bytes))?;

    if !bytes.is_empty() {
        return Err(DecodeError::custom(format_args!(
            "after decoding, there are {} extra bytes",
            bytes.len()
        )));
    }

    Ok(())
}

impl<const N: usize> BytesKey<N> {
    fn new(length: usize, bytes: [u8; N]) -> Self {
        let length = length as _;
        Self { length, bytes }
    }

    /// Decodes `self` as an [`AlgebraicValue`] at `key_type`.
    ///
    /// An incorrect `key_type`,
    /// i.e., one other than what was used when the index was created,
    /// may lead to a panic, but this is not guaranteed.
    /// The method could also silently succeed
    /// if the passed `key_type` incidentally happens to be compatible the stored bytes in `self`.
    pub(super) fn decode_algebraic_value(&self, key_type: &AlgebraicType) -> AlgebraicValue {
        AlgebraicValue::decode(key_type, &mut self.deref())
            .expect("A `BytesKey` should by construction always deserialize to the right `key_type`")
    }

    /// Decodes `bytes` in BSATN to a [`BytesKey<N>`]
    /// by copying over the bytes if they fit into the key.
    pub(super) fn from_bsatn(ty: &AlgebraicType, bytes: &[u8]) -> DecodeResult<Self> {
        // Validate the BSATN.
        validate(ty, bytes)?;
        // Copy the bytes over.
        let got = bytes.len();
        let mut arr = [0; N];
        arr[..got].copy_from_slice(bytes);
        Ok(Self::new(got, arr))
    }

    fn via_serializer(work: impl FnOnce(Serializer<'_, TeeWriter<&mut [u8], CountWriter>>)) -> Self {
        let mut bytes = [0; N];
        let (_, length) = CountWriter::run(bytes.as_mut_slice(), |writer| {
            let ser = Serializer::new(writer);
            work(ser)
        });
        Self::new(length, bytes)
    }

    /// Serializes the columns `cols` in `row_ref` to a [`BytesKey<N>`].
    ///
    /// It's assumed that `row_ref` projected to `cols`
    /// will fit into `N` bytes when serialized into BSATN.
    /// The method panics otherwise.
    ///
    /// SAFETY: Any `col` in `cols` is in-bounds of `row_ref`'s layout.
    pub(super) unsafe fn from_row_ref(cols: &ColList, row_ref: RowRef<'_>) -> Self {
        Self::via_serializer(|ser| {
            unsafe { row_ref.serialize_columns_unchecked(cols, ser) }
                .expect("should've serialized a `row_ref` to BSATN successfully");
        })
    }

    /// Serializes `av` to a [`BytesKey<N>`].
    ///
    /// It's assumed that `av`
    /// will fit into `N` bytes when serialized into BSATN.
    /// The method panics otherwise.
    pub(super) fn from_algebraic_value(av: &AlgebraicValue) -> Self {
        Self::via_serializer(|ser| {
            av.serialize_into_bsatn(ser)
                .expect("should've serialized an `AlgebraicValue` to BSATN successfully")
        })
    }
}

/// A key for an all-primitive multi-column index
/// serialized to a byte array.
///
/// These keys are derived from [`BytesKey`]
/// but are post-processed to work with ranges,
/// unlike the former type,
/// which only work with point indices (e.g., hash indices).
///
/// The post-processing converts how some types are stored in the encoding:
/// - unsigned integer types `uN`, where `N > 8` from little-endian to big-endian.
/// - signed integers are shifted such that `iN::MIN` is stored as `0`
///   and `iN:MAX` is stored as `uN::MAX`.
///
/// The `length` stores the number of actual bytes used by the key.
#[derive(Debug, Eq, Clone, Copy)]
pub(super) struct RangeCompatBytesKey<const N: usize> {
    length: u8,
    bytes: [u8; N],
}

impl<const N: usize> MemoryUsage for RangeCompatBytesKey<N> {}

impl<const N: usize> KeySize for RangeCompatBytesKey<N> {
    type MemoStorage = u64;

    fn key_size_in_bytes(&self) -> usize {
        self.length as usize
    }
}

impl<const N: usize> Deref for RangeCompatBytesKey<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes[0..self.length as usize]
    }
}

impl<const N: usize> PartialEq for RangeCompatBytesKey<N> {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl<const N: usize> PartialOrd for RangeCompatBytesKey<N> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Ord for RangeCompatBytesKey<N> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other.deref())
    }
}

impl<const N: usize> Hash for RangeCompatBytesKey<N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state);
    }
}

/// Splits `slice` into the first `N` bytes converting the former via `map_bytes`
/// and returning the rest.
fn split_map_write_back<const N: usize>(slice: &mut [u8], map_bytes: impl FnOnce([u8; N]) -> [u8; N]) -> &mut [u8] {
    let (bytes, rest) = slice.split_first_chunk_mut().unwrap();
    *bytes = map_bytes(*bytes);
    rest
}

impl<const N: usize> RangeCompatBytesKey<N> {
    fn new(length: usize, bytes: [u8; N]) -> Self {
        let length = length as _;
        Self { length, bytes }
    }

    /// Decodes `self` as an [`AlgebraicValue`] at `key_type`.
    ///
    /// An incorrect `key_type`,
    /// i.e., one other than what was used when the index was created,
    /// may lead to a panic, but this is not guaranteed.
    /// The method could also silently succeed
    /// if the passed `key_type` incidentally happens to be compatible the stored bytes in `self`.
    pub(super) fn decode_algebraic_value(&self, key_type: &AlgebraicType) -> AlgebraicValue {
        Self::to_bytes_key(*self, key_type).decode_algebraic_value(key_type)
    }

    /// Decodes `prefix` in BSATN to a [`RangeCompatBytesKey<N>`]
    /// by copying over `prefix` and massaging if they fit into the key.
    pub(super) fn from_bsatn_prefix(prefix: &[u8], prefix_types: &[ProductTypeElement]) -> DecodeResult<Self> {
        // Validate the BSATN.
        validate(prefix_types, prefix)?;

        // Copy the `prefix` over.
        let mut bytes = [0; N];
        let got = prefix.len();
        bytes[..got].copy_from_slice(prefix);

        // Massage the `bytes`.
        let mut slice = bytes.as_mut_slice();
        for ty in prefix_types {
            slice = Self::process_from_bytes_key(slice, &ty.algebraic_type);
        }

        Ok(Self::new(got, bytes))
    }

    /// Decodes `prefix` and `endpoint` in BSATN to a [`RangeCompatBytesKey<N>`]
    /// by copying over both and massaging if they fit into the key.
    pub(super) fn from_bsatn_prefix_and_endpoint(
        prefix: &[u8],
        prefix_types: &[ProductTypeElement],
        endpoint: &[u8],
        range_type: &AlgebraicType,
    ) -> DecodeResult<Self> {
        // Validate the BSATN.
        validate(prefix_types, prefix)?;
        validate(range_type, endpoint)?;

        // Sum up the lengths.
        let prefix_len = prefix.len();
        let endpoint_len = endpoint.len();
        let total_len = prefix_len + endpoint_len;

        // Copy the `prefix` and the `endpoint` over.
        let mut bytes = [0; N];
        bytes[..prefix_len].copy_from_slice(prefix);
        bytes[prefix_len..total_len].copy_from_slice(endpoint);

        // Massage the bytes.
        let mut slice = bytes.as_mut_slice();
        for ty in prefix_types {
            slice = Self::process_from_bytes_key(slice, &ty.algebraic_type);
        }
        Self::process_from_bytes_key(slice, range_type);

        Ok(Self::new(total_len, bytes))
    }

    /// Decodes `bytes` in BSATN to a [`RangeCompatBytesKey<N>`]
    /// by copying over the bytes if they fit into the key.
    pub(super) fn from_bsatn(ty: &AlgebraicType, bytes: &[u8]) -> DecodeResult<Self> {
        let key = BytesKey::from_bsatn(ty, bytes)?;
        Ok(Self::from_bytes_key(key, ty))
    }

    /// Serializes the columns `cols` in `row_ref` to a [`BytesKey<N>`].
    ///
    /// It's assumed that `row_ref` projected to `cols`
    /// will fit into `N` bytes when serialized into BSATN.
    /// The method panics otherwise.
    ///
    /// SAFETY: Any `col` in `cols` is in-bounds of `row_ref`'s layout.
    pub(super) unsafe fn from_row_ref(cols: &ColList, row_ref: RowRef<'_>, ty: &AlgebraicType) -> Self {
        // SAFETY: same as caller requirements.
        let key = unsafe { BytesKey::from_row_ref(cols, row_ref) };
        Self::from_bytes_key(key, ty)
    }

    /// Serializes `av` to a [`BytesKey<N>`].
    ///
    /// It's assumed that `av`
    /// will fit into `N` bytes when serialized into BSATN.
    /// The method panics otherwise.
    pub(super) fn from_algebraic_value(av: &AlgebraicValue, ty: &AlgebraicType) -> Self {
        let key = BytesKey::from_algebraic_value(av);
        Self::from_bytes_key(key, ty)
    }

    fn from_bytes_key(key: BytesKey<N>, ty: &AlgebraicType) -> Self {
        let BytesKey { length, mut bytes } = key;
        Self::process_from_bytes_key(bytes.as_mut_slice(), ty);
        Self { length, bytes }
    }

    fn process_from_bytes_key<'a>(mut slice: &'a mut [u8], ty: &AlgebraicType) -> &'a mut [u8] {
        use AlgebraicType::*;
        match ty {
            // For sums, read the tag and process the active variant.
            Sum(ty) => {
                let (&mut tag, rest) = slice.split_first_mut().unwrap();
                let ty = &ty.variants[tag as usize].algebraic_type;
                Self::process_from_bytes_key(rest, ty)
            }
            // For products, just process each field in sequence.
            Product(ty) => {
                for ty in &ty.elements {
                    slice = Self::process_from_bytes_key(slice, &ty.algebraic_type);
                }
                slice
            }
            // No need to do anything as these are only a single byte long.
            Bool | U8 => &mut slice[1..],
            // For unsigned integers, read them as LE and write back as BE.
            U16 => split_map_write_back(slice, |b| u16::from_le_bytes(b).to_be_bytes()),
            U32 => split_map_write_back(slice, |b| u32::from_le_bytes(b).to_be_bytes()),
            U64 => split_map_write_back(slice, |b| u64::from_le_bytes(b).to_be_bytes()),
            U128 => split_map_write_back(slice, |b| u128::from_le_bytes(b).to_be_bytes()),
            U256 => split_map_write_back(slice, |b| u256::from_le_bytes(b).to_be_bytes()),
            // For signed integers, read them as LE, make them unsigned, and write back as BE.
            I8 => split_map_write_back(slice, |b| i8::from_le_bytes(b).wrapping_sub(i8::MIN).to_be_bytes()),
            I16 => split_map_write_back(slice, |b| i16::from_le_bytes(b).wrapping_sub(i16::MIN).to_be_bytes()),
            I32 => split_map_write_back(slice, |b| i32::from_le_bytes(b).wrapping_sub(i32::MIN).to_be_bytes()),
            I64 => split_map_write_back(slice, |b| i64::from_le_bytes(b).wrapping_sub(i64::MIN).to_be_bytes()),
            I128 => split_map_write_back(slice, |b| i128::from_le_bytes(b).wrapping_sub(i128::MIN).to_be_bytes()),
            I256 => split_map_write_back(slice, |b| i256::from_le_bytes(b).wrapping_sub(i256::MIN).to_be_bytes()),
            // Refs don't exist here and
            // arrays and strings are of unbounded length.
            // For floats, we haven't considred them yet.
            Ref(_) | Array(_) | String | F32 | F64 => unreachable!(),
        }
    }

    fn to_bytes_key(key: Self, ty: &AlgebraicType) -> BytesKey<N> {
        fn process<'a>(mut slice: &'a mut [u8], ty: &AlgebraicType) -> &'a mut [u8] {
            use AlgebraicType::*;
            match ty {
                // For sums, read the tag and process the active variant.
                Sum(ty) => {
                    let (&mut tag, rest) = slice.split_first_mut().unwrap();
                    let ty = &ty.variants[tag as usize].algebraic_type;
                    process(rest, ty)
                }
                // For products, just process each field in sequence.
                Product(ty) => {
                    for ty in &ty.elements {
                        slice = process(slice, &ty.algebraic_type);
                    }
                    slice
                }
                // No need to do anything as these are only a single byte long.
                Bool | U8 => &mut slice[1..],
                // For unsigned integers, read them as BE and write back as LE.
                U16 => split_map_write_back(slice, |b| u16::from_be_bytes(b).to_le_bytes()),
                U32 => split_map_write_back(slice, |b| u32::from_be_bytes(b).to_le_bytes()),
                U64 => split_map_write_back(slice, |b| u64::from_be_bytes(b).to_le_bytes()),
                U128 => split_map_write_back(slice, |b| u128::from_be_bytes(b).to_le_bytes()),
                U256 => split_map_write_back(slice, |b| u256::from_be_bytes(b).to_le_bytes()),
                // For signed integers, read them as LE, make them unsigned, and write back as BE.
                I8 => split_map_write_back(slice, |b| i8::from_be_bytes(b).wrapping_add(i8::MIN).to_le_bytes()),
                I16 => split_map_write_back(slice, |b| i16::from_be_bytes(b).wrapping_add(i16::MIN).to_le_bytes()),
                I32 => split_map_write_back(slice, |b| i32::from_be_bytes(b).wrapping_add(i32::MIN).to_le_bytes()),
                I64 => split_map_write_back(slice, |b| i64::from_be_bytes(b).wrapping_add(i64::MIN).to_le_bytes()),
                I128 => split_map_write_back(slice, |b| i128::from_be_bytes(b).wrapping_add(i128::MIN).to_le_bytes()),
                I256 => split_map_write_back(slice, |b| i256::from_be_bytes(b).wrapping_add(i256::MIN).to_le_bytes()),
                // Refs don't exist here and
                // arrays and strings are of unbounded length.
                // For floats, we haven't considred them yet.
                Ref(_) | Array(_) | String | F32 | F64 => unreachable!(),
            }
        }

        let Self { length, mut bytes } = key;
        process(bytes.as_mut_slice(), ty);
        BytesKey { length, bytes }
    }

    /// Extend the length to `N` by filling with `u8::MAX`.
    pub(super) fn add_max_suffix(mut self) -> Self {
        let len = self.len();
        self.bytes[len..].fill(u8::MAX);
        self.length = N as u8;
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::array::uniform;
    use proptest::prelude::*;
    use spacetimedb_sats::bsatn::to_len;
    use spacetimedb_sats::proptest::{gen_with, generate_product_value, generate_row_type, generate_typed_row, SIZE};

    const N: usize = u8::MAX as usize;

    proptest! {
        #![proptest_config(ProptestConfig { max_global_rejects: 65536, ..<_>::default() })]

        #[test]
        fn test_bytes_key_round_trip((ty, av) in generate_typed_row()) {
            let len = to_len(&av).unwrap();
            prop_assume!(len <= N);

            let ty = AlgebraicType::Product(ty);
            let av = AlgebraicValue::Product(av);
            let key = BytesKey::<N>::from_algebraic_value(&av);
            let decoded_av = key.decode_algebraic_value(&ty);
            prop_assert_eq!(av, decoded_av);
        }

        /// This test does not hold for `BytesKey`
        /// as BSATN stores them little-endian,
        /// but `Ord for AlgebraicValue` compares them as big-endian.
        /// It does however hold for `RangeCompatBytesKey` which
        /// massages the BSATN to make it order-preserving.
        #[test]
        fn order_in_bsatn_is_preserved((ty, [r1, r2]) in gen_with(generate_row_type(0..=SIZE), |ty| uniform(generate_product_value(ty)))) {
            let ty: AlgebraicType = ty.into();
            let r1: AlgebraicValue = r1.into();
            let r2: AlgebraicValue = r2.into();

            let Some(required) = required_bytes_key_size(&ty, true) else {
                return Err(TestCaseError::reject("type is incompatible with fixed byte keys in range indices"));
            };
            prop_assume!(required <= N);

            let k1 = BytesKey::<N>::from_algebraic_value(&r1);
            let kr1 = RangeCompatBytesKey::from_bytes_key(k1, &ty);
            let k2 = BytesKey::<N>::from_algebraic_value(&r2);
            let kr2 = RangeCompatBytesKey::from_bytes_key(k2, &ty);
            let ord_kr = kr1.cmp(&kr2);
            let ord_r = r1.cmp(&r2);
            prop_assert_eq!(ord_kr, ord_r);
        }
    }
}
