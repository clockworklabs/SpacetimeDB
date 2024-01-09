//! Provides the function [`hash_row_in_page(hasher, page, fixed_offset, ty)`]
//! which hashes `value = page.get_row_data(fixed_offset, fixed_row_size)` typed at `ty`
//! and associated var len objects in `value` into `hasher`.

use super::{
    de::{read_tag, read_vlr},
    indexes::{Bytes, PageOffset},
    layout::{align_to, AlgebraicTypeLayout, HasLayout, ProductTypeLayout, RowTypeLayout},
    page::Page,
    util::{range_add, slice_assume_init_ref},
};
use core::hash::Hasher;

/// Hashes the row in `page` where the fixed part starts at `fixed_offset`
/// and lasts `ty.size()` bytes. This region is typed at `ty`.
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

/// Hashes every product field in `value = &bytes[range_add(0..ty.size(), *curr_offset)]`
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
        hash_value(hasher, bytes, page, curr_offset, &elem_ty.ty);
    }
}

/// Hashes `value = &bytes[range_add(0..ty.size(), *curr_offset)]` typed at `ty`
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
            // we know `data_value = &bytes[range_add(0..data_ty.size(), data_offset))`
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
        &AlgebraicTypeLayout::Bool
        | &AlgebraicTypeLayout::I8
        | &AlgebraicTypeLayout::U8
        | &AlgebraicTypeLayout::I16
        | &AlgebraicTypeLayout::U16
        | &AlgebraicTypeLayout::I32
        | &AlgebraicTypeLayout::U32
        | &AlgebraicTypeLayout::I64
        | &AlgebraicTypeLayout::U64
        | &AlgebraicTypeLayout::I128
        | &AlgebraicTypeLayout::U128
        | &AlgebraicTypeLayout::F32
        | &AlgebraicTypeLayout::F64 => {
            // SAFETY: `value` was valid,
            // so `&bytes[range_add(0..ty.size(), *curr_offset)]` contains init bytes.
            unsafe { hash_byte_array(hasher, bytes, curr_offset, ty.size()) }
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
/// SAFETY: `data = bytes[range_add(0..size_of::<VarLenRef>(), *curr_offset)]`
/// must be a valid `vlr = VarLenRef` and `&data` must be properly aligned for a `VarLenRef`.
/// The `vlr.first_granule` must be `NULL` or must point to a valid granule in `page`.
unsafe fn hash_vlo(hasher: &mut impl Hasher, page: &Page, bytes: &Bytes, curr_offset: &mut usize) {
    // SAFETY: We have a valid `VarLenRef` at `&data`.
    let vlr = unsafe { read_vlr(bytes, curr_offset) };
    // SAFETY: ^-- got valid `VarLenRef` where `vlr.first_granule` was `NULL`
    // or a pointer to a valid starting granule, as required.
    for data in unsafe { page.iter_vlo_data(vlr.first_granule) } {
        hasher.write(data);
    }
}

/// Hashes the byte array `data = &bytes[range_move(0..len, *curr_offset)]`
/// and advances the offset.
///
/// SAFETY: `data` must be initialized as a valid `&[u8]`.
unsafe fn hash_byte_array(hasher: &mut impl Hasher, bytes: &Bytes, curr_offset: &mut usize, len: usize) {
    let data = &bytes[range_add(0..len, *curr_offset)];
    // SAFETY: Caller promised that `data` was initialized.
    let data = unsafe { slice_assume_init_ref(data) };
    hasher.write(data);
    *curr_offset += len;
}
