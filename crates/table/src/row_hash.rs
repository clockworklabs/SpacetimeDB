//! Provides the function [`hash_row_in_page(hasher, page, fixed_offset, ty)`]
//! which hashes `value = page.get_row_data(fixed_offset, fixed_row_size)` typed at `ty`
//! and associated var len objects in `value` into `hasher`.

use super::{
    bflatn_from::read_tag,
    indexes::{Bytes, PageOffset},
    layout::{align_to, AlgebraicTypeLayout, HasLayout, ProductTypeLayout, RowTypeLayout},
    page::Page,
    var_len::VarLenRef,
};
use core::hash::{Hash as _, Hasher};
use core::mem;
use spacetimedb_sats::{F32, F64};

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
///   `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
pub unsafe fn hash_row_in_page(hasher: &mut impl Hasher, page: &Page, fixed_offset: PageOffset, ty: &RowTypeLayout) {
    let fixed_bytes = page.get_row_data(fixed_offset, ty.size());

    // SAFETY:
    // - Per 1. and 2., `fixed_bytes` points at a row in `page` valid for `ty`.
    // - Per 3., for any `vlr: VarLenRef` stored in `fixed_bytes`,
    //   `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
    unsafe { hash_product(hasher, fixed_bytes, page, &mut 0, ty.product()) };
}

/// Hashes every product field in `value = &bytes[range_move(0..ty.size(), *curr_offset)]`
/// which is typed at `ty`.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///   `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn hash_product(
    hasher: &mut impl Hasher,
    bytes: &Bytes,
    page: &Page,
    curr_offset: &mut usize,
    ty: &ProductTypeLayout,
) {
    for elem_ty in &*ty.elements {
        // SAFETY: By 1., `value` is valid at `ty`,
        // so it follows that valid and properly aligned sub-`value`s
        // are valid `elem_ty.ty`s.
        // By 2., and the above, it follows that sub-`value`s won't have dangling `VarLenRef`s.
        unsafe {
            hash_value(hasher, bytes, page, curr_offset, &elem_ty.ty);
        }
    }
}

/// Hashes `value = &bytes[range_move(0..ty.size(), *curr_offset)]` typed at `ty`
/// and advances the `curr_offset`.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///   `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn hash_value(
    hasher: &mut impl Hasher,
    bytes: &Bytes,
    page: &Page,
    curr_offset: &mut usize,
    ty: &AlgebraicTypeLayout,
) {
    let ty_alignment = ty.align();
    *curr_offset = align_to(*curr_offset, ty_alignment);

    match ty {
        AlgebraicTypeLayout::Sum(ty) => {
            // Read the tag of the sum value.
            // SAFETY: `bytes[curr_offset..]` hold a sum value at `ty`.
            let (tag, data_ty) = unsafe { read_tag(bytes, ty, *curr_offset) };

            // Hash the variant data value.
            let mut data_offset = *curr_offset + ty.offset_of_variant_data(tag);
            // SAFETY: `value` is valid at `ty` so given `tag`,
            // we know `data_value = &bytes[range_move(0..data_ty.size(), data_offset))`
            // is valid at `data_ty`.
            // By 2., and the above, we also know that `data_value` won't have dangling `VarLenRef`s.
            unsafe { hash_value(hasher, bytes, page, &mut data_offset, data_ty) };
            *curr_offset += ty.size();
        }
        AlgebraicTypeLayout::Product(ty) => {
            // SAFETY: `value` was valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { hash_product(hasher, bytes, page, curr_offset, ty) }
        }

        // The primitive types:
        //
        // SAFETY (applies to app primitive types):
        // Per caller requirement, know `value` points to a valid `ty`.
        // Thus `&bytes[range_move(0..ty.size(), *curr_offset)]` points to init bytes
        // and `ty.size()` corresponds exactly to `N = 1, 1, 1, 2, 2, 4, 4, 8, 8, 16, 16, 4, 8`.
        &AlgebraicTypeLayout::Bool | &AlgebraicTypeLayout::U8 => {
            hasher.write_u8(unsafe { read_from_bytes::<u8>(bytes, curr_offset) })
        }
        &AlgebraicTypeLayout::I8 => hasher.write_i8(unsafe { read_from_bytes(bytes, curr_offset) }),
        &AlgebraicTypeLayout::I16 => {
            hasher.write_i16(i16::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U16 => {
            hasher.write_u16(u16::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::I32 => {
            hasher.write_i32(i32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U32 => {
            hasher.write_u32(u32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::I64 => {
            hasher.write_i64(i64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U64 => {
            hasher.write_u64(u64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::I128 => {
            hasher.write_i128(i128::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U128 => {
            hasher.write_u128(u128::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::F32 => {
            F32::from(f32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) })).hash(hasher)
        }
        &AlgebraicTypeLayout::F64 => {
            F64::from(f64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) })).hash(hasher)
        }

        // The var-len cases.
        &AlgebraicTypeLayout::String | AlgebraicTypeLayout::VarLen(_) => {
            // SAFETY: `value` was valid at and aligned for `ty`.
            // These `ty` store a `vlr: VarLenRef` as their value,
            // so the range is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr.first_granule` was promised by the caller
            // to either be `NULL` or point to a valid granule in `page`.
            unsafe { hash_vlo(hasher, page, bytes, curr_offset) }
        }
    }
    // TODO(perf,bikeshedding): unncessary work for some cases?
    *curr_offset = align_to(*curr_offset, ty_alignment);
}

/// Hashes the bytes of a var-len object
/// referred to at by the var-len reference at `curr_offset`
/// which is then advanced.
///
/// The function does not care about large-blob-ness.
/// Rather, the blob hash is implicitly hashed.
///
/// SAFETY: `data = bytes[range_move(0..size_of::<VarLenRef>(), *curr_offset)]`
/// must be a valid `vlr = VarLenRef` and `&data` must be properly aligned for a `VarLenRef`.
/// The `vlr.first_granule` must be `NULL` or must point to a valid granule in `page`.
unsafe fn hash_vlo(hasher: &mut impl Hasher, page: &Page, bytes: &Bytes, curr_offset: &mut usize) {
    // SAFETY: We have a valid `VarLenRef` at `&data`.
    let vlr = unsafe { read_from_bytes::<VarLenRef>(bytes, curr_offset) };
    // SAFETY: ^-- got valid `VarLenRef` where `vlr.first_granule` was `NULL`
    // or a pointer to a valid starting granule, as required.
    for data in unsafe { page.iter_vlo_data(vlr.first_granule) } {
        hasher.write(data);
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
    let ptr: *const T = bytes.as_ptr().cast();
    // SAFETY: Caller promised that `ptr` points to a `T`.
    // Moreover, `ptr` is derived from a shared reference with permission to read this range.
    unsafe { *ptr }
}
