//! Provides the function [`hash_row_in_page(hasher, page, fixed_offset, ty)`]
//! which hashes `value = page.get_row_data(fixed_offset, fixed_row_size)` typed at `ty`
//! and associated var len objects in `value` into `hasher`.

use super::{
    bflatn_from::read_tag,
    indexes::{Bytes, PageOffset},
    layout::{align_to, AlgebraicTypeLayout, HasLayout, RowTypeLayout},
    page::Page,
    var_len::VarLenRef,
};
use crate::{
    bflatn_from::vlr_blob_bytes,
    blob_store::BlobStore,
    layout::{ProductTypeLayoutView, VarLenType},
};
use core::hash::{Hash as _, Hasher};
use core::mem;
use core::str;
use spacetimedb_sats::{algebraic_value::ser::concat_byte_chunks_buf, bsatn::Deserializer, i256, u256, F32, F64};

/// Hashes the row in `page` where the fixed part starts at `fixed_offset`
/// and lasts `ty.size()` bytes. This region is typed at `ty`.
///
/// Note that the hash of an in-page row might not be the same as
/// hashing the row as its equivalent `ProductValue`.
///
/// # Safety
///
/// 1. the `fixed_offset` must point at a row in `page` lasting `ty.size()` bytes.
/// 2. the row must be a valid `ty`.
/// 3. for any `vlr: VarLenRef` stored in the row,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
pub unsafe fn hash_row_in_page(
    hasher: &mut impl Hasher,
    page: &Page,
    blob_store: &dyn BlobStore,
    fixed_offset: PageOffset,
    ty: &RowTypeLayout,
) {
    let fixed_bytes = page.get_row_data(fixed_offset, ty.size());

    // SAFETY:
    // - Per 1. and 2., `fixed_bytes` points at a row in `page` valid for `ty`.
    // - Per 3., for any `vlr: VarLenRef` stored in `fixed_bytes`,
    //   `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
    unsafe { hash_product(hasher, fixed_bytes, page, blob_store, &mut 0, ty.product()) };
}

/// Hashes every product field in `value = &bytes[range_move(0..ty.size(), *curr_offset)]`
/// which is typed at `ty`.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn hash_product(
    hasher: &mut impl Hasher,
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: &mut usize,
    ty: ProductTypeLayoutView<'_>,
) {
    let base_offset = *curr_offset;
    for elem_ty in ty.elements {
        *curr_offset = base_offset + elem_ty.offset as usize;

        // SAFETY: By 1., `value` is valid at `ty`,
        // so it follows that valid and properly aligned sub-`value`s
        // are valid `elem_ty.ty`s.
        // By 2., and the above, it follows that sub-`value`s won't have dangling `VarLenRef`s.
        unsafe {
            hash_value(hasher, bytes, page, blob_store, curr_offset, &elem_ty.ty);
        }
    }
}

/// Hashes `value = &bytes[range_move(0..ty.size(), *curr_offset)]` typed at `ty`
/// and advances the `curr_offset`.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn hash_value(
    hasher: &mut impl Hasher,
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: &mut usize,
    ty: &AlgebraicTypeLayout,
) {
    debug_assert_eq!(
        *curr_offset,
        align_to(*curr_offset, ty.align()),
        "curr_offset {} insufficiently aligned for type {:?}",
        *curr_offset,
        ty
    );

    match ty {
        AlgebraicTypeLayout::Sum(ty) => {
            // Read and hash the tag of the sum value.
            let (tag, data_ty) = read_tag(bytes, ty, *curr_offset);
            tag.hash(hasher);

            // Hash the variant data value.
            let mut data_offset = *curr_offset + ty.offset_of_variant_data(tag);
            // SAFETY: `value` is valid at `ty` so given `tag`,
            // we know `data_value = &bytes[range_move(0..data_ty.size(), data_offset))`
            // is valid at `data_ty`.
            // By 2., and the above, we also know that `data_value` won't have dangling `VarLenRef`s.
            unsafe { hash_value(hasher, bytes, page, blob_store, &mut data_offset, data_ty) };
            *curr_offset += ty.size();
        }
        AlgebraicTypeLayout::Product(ty) => {
            // SAFETY: `value` was valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { hash_product(hasher, bytes, page, blob_store, curr_offset, ty.view()) }
        }

        // The primitive types:
        //
        // SAFETY (applies to app primitive types):
        // Per caller requirement, know `value` points to a valid `ty`.
        // Thus `&bytes[range_move(0..ty.size(), *curr_offset)]` points to init bytes
        // and `ty.size()` corresponds exactly to `N = 1, 1, 1, 2, 2, 4, 4, 8, 8, 16, 16, 32, 32, 4, 8`.
        &AlgebraicTypeLayout::Bool | &AlgebraicTypeLayout::U8 => {
            u8::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher)
        }
        &AlgebraicTypeLayout::I8 => i8::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::I16 => i16::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::U16 => u16::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::I32 => i32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::U32 => u32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::I64 => i64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::U64 => u64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::I128 => i128::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::U128 => u128::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::I256 => i256::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::U256 => u256::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }).hash(hasher),
        &AlgebraicTypeLayout::F32 => {
            F32::from(f32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) })).hash(hasher)
        }
        &AlgebraicTypeLayout::F64 => {
            F64::from(f64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) })).hash(hasher)
        }

        // The var-len cases.
        &AlgebraicTypeLayout::String => {
            // SAFETY: `value` was valid at and aligned for `ty`.
            // These `ty` store a `vlr: VarLenRef` as their value,
            // so the range is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr.first_granule` was promised by the caller
            // to either be `NULL` or point to a valid granule in `page`.
            unsafe {
                run_vlo_bytes(page, bytes, blob_store, curr_offset, |bytes| {
                    // SAFETY: For `::String`, the blob will always be valid UTF-8.
                    let string = str::from_utf8_unchecked(bytes);
                    string.hash(hasher)
                });
            }
        }
        AlgebraicTypeLayout::VarLen(VarLenType::Array(ty)) => {
            // SAFETY: `value` was valid at and aligned for `ty`.
            // These `ty` store a `vlr: VarLenRef` as their value,
            // so the range is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr.first_granule` was promised by the caller
            // to either be `NULL` or point to a valid granule in `page`.
            unsafe {
                run_vlo_bytes(page, bytes, blob_store, curr_offset, |mut bsatn| {
                    let de = Deserializer::new(&mut bsatn);
                    spacetimedb_sats::hash_bsatn(hasher, ty, de).unwrap();
                });
            }
        }
    }
}

/// Runs the function `run` on the concatenated bytes of a var-len object,
/// referred to at by the var-len reference at `curr_offset`
/// which is then advanced.
///
/// SAFETY: `data = bytes[range_move(0..size_of::<VarLenRef>(), *curr_offset)]`
/// must be a valid `vlr = VarLenRef` and `&data` must be properly aligned for a `VarLenRef`.
/// The `vlr.first_granule` must be `NULL` or must point to a valid granule in `page`.
pub(crate) unsafe fn run_vlo_bytes<R>(
    page: &Page,
    bytes: &Bytes,
    blob_store: &dyn BlobStore,
    curr_offset: &mut usize,
    run: impl FnOnce(&[u8]) -> R,
) -> R {
    // SAFETY: `value` was valid at and aligned for `ty`.
    // These `ty` store a `vlr: VarLenRef` as their fixed value.
    // The range thus is valid and properly aligned for `VarLenRef`.
    let vlr = unsafe { read_from_bytes::<VarLenRef>(bytes, curr_offset) };

    if vlr.is_large_blob() {
        // SAFETY: As `vlr` is a blob, `vlr.first_granule` always points to a valid granule.
        let bytes = unsafe { vlr_blob_bytes(page, blob_store, vlr) };
        run(bytes)
    } else {
        // SAFETY: `vlr.first_granule` is either NULL or points to a valid granule.
        let var_iter = unsafe { page.iter_vlo_data(vlr.first_granule) };
        let total_len = vlr.length_in_bytes as usize;

        // SAFETY: `total_len == var_iter.map(|c| c.len()).sum()`.
        unsafe { concat_byte_chunks_buf(total_len, var_iter, run) }
    }
}

/// Read a `T` from `bytes` at the `curr_offset` and advance by `size` bytes.
///
/// # Safety
///
/// Let `value = &bytes[range_move(0..size_of::<T>(), *curr_offset)]`.
/// Then `value` must point to a valid `T` and must be properly aligned for `T`.
pub unsafe fn read_from_bytes<T: Copy>(bytes: &Bytes, curr_offset: &mut usize) -> T {
    let bytes = &bytes[*curr_offset..];
    *curr_offset += mem::size_of::<T>();
    // TODO: Endianness concerns? Do we need to explicitly read as little-endian here?
    let ptr: *const T = bytes.as_ptr().cast();
    // SAFETY: Caller promised that `ptr` points to a `T`.
    // Moreover, `ptr` is derived from a shared reference with permission to read this range.
    unsafe { *ptr }
}

#[cfg(test)]
mod tests {
    use crate::{blob_store::HashMapBlobStore, page_pool::PagePool};
    use core::hash::BuildHasher;
    use proptest::prelude::*;
    use spacetimedb_sats::proptest::generate_typed_row;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(if cfg!(miri) { 8 } else { 2048 }))]
        #[test]
        fn pv_row_ref_hash_same_std_random_state((ty, val) in generate_typed_row()) {
            // Turn `val` into a `RowRef`.
            let mut table = crate::table::test::table(ty);
            let pool = &PagePool::new_for_test();
            let blob_store = &mut HashMapBlobStore::default();
            let (_, row) = table.insert(pool, blob_store, &val).unwrap();

            // Check hashing algos.
            let rs = std::hash::RandomState::new();
            prop_assert_eq!(rs.hash_one(&val), rs.hash_one(row));
        }

        #[test]
        fn pv_row_ref_hash_same_ahash((ty, val) in generate_typed_row()) {
            // Turn `val` into a `RowRef`.
            let pool = &PagePool::new_for_test();
            let blob_store = &mut HashMapBlobStore::default();
            let mut table = crate::table::test::table(ty);
            let (_, row) = table.insert(pool, blob_store, &val).unwrap();

            // Check hashing algos.
            let rs = std::hash::RandomState::new();
            prop_assert_eq!(rs.hash_one(&val), rs.hash_one(row));
        }
    }
}
