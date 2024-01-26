//! Provides the function [`read_row_from_page(hasher, page, fixed_offset, ty)`]
//! which reads `value = page.get_row_data(fixed_offset, fixed_row_size)` typed at `ty`
//! and associated var len objects in `value` into a `ProductValue`.

use super::{
    blob_store::BlobStore,
    indexes::{Bytes, PageOffset},
    layout::{
        align_to, AlgebraicTypeLayout, HasLayout as _, ProductTypeLayout, RowTypeLayout, SumTypeLayout, VarLenType,
    },
    page::Page,
    var_len::VarLenRef,
};
use core::mem;
use spacetimedb_sats::{AlgebraicValue, ProductValue};

/// Reads the row in `page` where the fixed part starts at `fixed_offset`
/// and lasts `ty.size()` bytes. This region is typed at `ty`.
///
/// # Safety
///
/// 1. the `fixed_offset` must point at a row in `page` lasting `ty.size()` byte.
/// 2. the row must be a valid `ty`.
/// 3. for any `vlr: VarLenRef` stored in the row,
///   `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
pub unsafe fn read_row_from_page(
    page: &Page,
    blob_store: &dyn BlobStore,
    fixed_offset: PageOffset,
    ty: &RowTypeLayout,
) -> ProductValue {
    let fixed_bytes = page.get_row_data(fixed_offset, ty.size());
    // SAFETY:
    // - Per 1. and 2., `fixed_bytes` points at a row in `page` valid for `ty`.
    // - Per 3., for any `vlr: VarLenRef` stored in `fixed_bytes`,
    //   `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
    unsafe { deserialize_product(fixed_bytes, page, blob_store, &mut 0, ty.product()) }
}

/// Reads every product field in `value = &bytes[range_move(0..ty.size(), *curr_offset)]`,
/// which is typed at `ty`,
/// and then stitches together.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///   `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn deserialize_product(
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: &mut usize,
    ty: &ProductTypeLayout,
) -> ProductValue {
    let elements = ty
        .elements
        .iter()
        // SAFETY: By 1., `value` is valid at `ty`,
        // so it follows that valid and properly aligned sub-`value`s
        // are valid `elem_ty.ty`s.
        // By 2., and the above, it follows that sub-`value`s won't have dangling `VarLenRef`s.
        .map(|elem_ty| unsafe { deserialize_value(bytes, page, blob_store, curr_offset, &elem_ty.ty) })
        .collect::<Vec<AlgebraicValue>>();
    ProductValue { elements }
}

/// Reads the tag of the sum value and selects the data variant type.
///
/// # Safety
///
/// `bytes[curr_offset..]` has a sum value typed at `ty`.
pub unsafe fn read_tag<'ty>(
    bytes: &Bytes,
    ty: &'ty SumTypeLayout,
    curr_offset: usize,
) -> (u8, &'ty AlgebraicTypeLayout) {
    let tag_offset = ty.offset_of_tag();
    let tag = bytes[curr_offset + tag_offset];
    // SAFETY: Caller promised that `bytes[curr_offset..]` has a sum value typed at `ty`.
    // We can therefore assume that `curr_offset + tag_offset` refers to a valid `u8`.
    let tag = unsafe { tag.assume_init() };

    // Extract the variant data type depending on the tag.
    let data_ty = &ty.variants[tag as usize].ty;

    (tag, data_ty)
}

/// Reads `value = &bytes[range_move(0..ty.size(), *curr_offset)]`
/// into an `AlgebraicValue` typed at `ty`
/// using `blob_store` to retreive the bytes of any large blob object
/// and `curr_offset`, then advanced, to decide where to start reading.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///   `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn deserialize_value(
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: &mut usize,
    ty: &AlgebraicTypeLayout,
) -> AlgebraicValue {
    let ty_alignment = ty.align();
    *curr_offset = align_to(*curr_offset, ty_alignment);

    let res = match ty {
        AlgebraicTypeLayout::Sum(ty) => {
            // Read the tag of the sum value.
            // SAFETY: `bytes[curr_offset..]` hold a sum value at `ty`.
            let (tag, data_ty) = unsafe { read_tag(bytes, ty, *curr_offset) };

            // Read the variant data value.
            let mut data_offset = *curr_offset + ty.offset_of_variant_data(tag);
            // SAFETY: `value` is valid at `ty` so given `tag`,
            // we know `data_value = &bytes[range_move(0..data_ty.size(), data_offset))`
            // is valid at `data_ty`.
            // By 2., and the above, we also know that `data_value` won't have dangling `VarLenRef`s.
            let data_av = unsafe { deserialize_value(bytes, page, blob_store, &mut data_offset, data_ty) };
            *curr_offset += ty.size();

            // Stitch together.
            AlgebraicValue::sum(tag, data_av)
        }
        AlgebraicTypeLayout::Product(ty) => {
            // SAFETY: `value` was valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { deserialize_product(bytes, page, blob_store, curr_offset, ty) }.into()
        }
        // The primitive types:
        //
        // SAFETY (applies to app primitive types):
        // Per caller requirement, know `value` points to a valid `ty`.
        // Thus `&bytes[range_move(0..ty.size(), *curr_offset)]` points to init bytes
        // and `ty.size()` corresponds exactly to `N = 1, 1, 1, 2, 2, 4, 4, 8, 8, 16, 16, 4, 8`.
        &AlgebraicTypeLayout::Bool => (unsafe { read_u8_unchecked(bytes, curr_offset) } != 0).into(),
        &AlgebraicTypeLayout::I8 => (unsafe { read_u8_unchecked(bytes, curr_offset) } as i8).into(),
        &AlgebraicTypeLayout::U8 => unsafe { read_u8_unchecked(bytes, curr_offset) }.into(),
        &AlgebraicTypeLayout::I16 => i16::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::U16 => u16::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::I32 => i32::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::U32 => u32::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::I64 => i64::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::U64 => u64::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::I128 => i128::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::U128 => u128::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::F32 => f32::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),
        &AlgebraicTypeLayout::F64 => f64::from_le_bytes(unsafe { read_byte_array(bytes, curr_offset) }).into(),

        // The var-len cases.
        &AlgebraicTypeLayout::String => {
            // SAFETY: `value` was valid at and aligned for `::String`
            // which stores a `vlr: VarLenRef` as its fixed value.
            // The range thus is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr.first_granule` was promised by the caller
            // to either be `NULL` or point to a valid granule in `page`.
            let bytes = unsafe { read_vlr_to_bytes(page, blob_store, bytes, curr_offset) };
            let string = String::from_utf8(bytes).unwrap();
            string.into()
        }
        AlgebraicTypeLayout::VarLen(VarLenType::Array(ty) | VarLenType::Map(ty)) => {
            // SAFETY: `value` was valid at and aligned for `ty`.
            // These `ty` store a `vlr: VarLenRef` as their fixed value.
            // The range thus is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr.first_granule` was promised by the caller
            // to either be `NULL` or point to a valid granule in `page`.
            let bytes = unsafe { read_vlr_to_bytes(page, blob_store, bytes, curr_offset) };
            AlgebraicValue::decode(ty, &mut &*bytes).unwrap()
        }
    };
    // TODO(perf,bikeshedding): unncessary work for some cases?
    *curr_offset = align_to(*curr_offset, ty_alignment);
    res
}

/// Read the bytes of a var-len object
/// referred to at by the var-len reference at the current offset.
///
/// SAFETY: (1) `data = &bytes[range_move(0..size_of::<VarLenRef>(), *curr_offset)]`
/// must point to a valid `VarLenRef` and `data` must be properly aligned for a `VarLenRef`.
/// (2) The `vlr.first_granule` must be `NULL` or must point to a valid granule in `page`.
unsafe fn read_vlr_to_bytes(
    page: &Page,
    blob_store: &dyn BlobStore,
    bytes: &Bytes,
    curr_offset: &mut usize,
) -> Vec<u8> {
    // SAFETY: (1) We have a valid `VarLenRef` at `&data`.
    let vlr = unsafe { read_vlr(bytes, curr_offset) };
    if vlr.is_large_blob() {
        // SAFETY: (2)
        unsafe { read_vec_from_blob(page, blob_store, vlr) }
    } else {
        // SAFETY: (2)
        unsafe { read_vec(page, vlr) }
    }
}

/// Read the bytes of the var-len object referred to by `vlr`.
///
/// SAFETY: (1) `vlr.first_granule` must be `NULL` or must point to a valid granule in `page`.
unsafe fn read_vec(page: &Page, vlr: VarLenRef) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vlr.length_in_bytes as usize);
    // An initialized var-len object is either a UTF-8 string or BSATN-encoded,
    // and in either case is fully init.
    //
    // SAFETY: (1)
    let var_iter = unsafe { page.iter_vlo_data(vlr.first_granule) };
    for granule in var_iter {
        bytes.extend(granule);
    }
    bytes
}

/// Read a large blob object from `vlr.first_granule`.
///
/// SAFETY: (1) `vlr.first_granule` must be `NULL` or must point to a valid granule in `page`.
#[cold]
#[inline(never)]
unsafe fn read_vec_from_blob(page: &Page, blob_store: &dyn BlobStore, vlr: VarLenRef) -> Vec<u8> {
    // SAFETY: (1)
    let mut var_iter = unsafe { page.iter_var_len_object(vlr.first_granule) };
    let granule = var_iter.next().unwrap();
    let hash = granule.blob_hash();
    blob_store.retrieve_blob(&hash).unwrap().to_owned()
}

/// Read a [`VarLenRef`] from `bytes` at the `curr_offset` and advance.
///
/// # Safety
///
/// `data = &bytes[range_move(0..size_of::<VarLenRef>(), *curr_offset)]`
/// must point to a valid `VarLenRef` and `data` must be properly aligned for a `VarLenRef`.
pub unsafe fn read_vlr(bytes: &Bytes, curr_offset: &mut usize) -> VarLenRef {
    // SAFETY: `T = VarLenRef` and caller promised that `data` points to a valid `VarLenRef`.
    unsafe { read_from_bytes(bytes, curr_offset) }
}

/// Read a byte from `bytes` at the `curr_offset` and advance.
///
/// # Safety
///
/// Let `value = &bytes[range_move(0..1, *curr_offset)]`.
/// Then `value` must point to a valid `u8`.
/// Proper alignment is already trivially satisfied as `align_of::<u8>() == 1`.
pub unsafe fn read_u8_unchecked(bytes: &Bytes, curr_offset: &mut usize) -> u8 {
    unsafe { read_byte_array::<1>(bytes, curr_offset)[0] }
}

/// Read a byte array of `N` elements from `bytes` at the `curr_offset` and advance.
///
/// # Safety
///
/// Let `value = &bytes[range_move(0..N, *curr_offset)]`.
/// Then `value` must point to a valid `[u8; N]`.
/// Proper alignment is already trivially satisfied as `align_of::<u8>() == 1`.
pub unsafe fn read_byte_array<const N: usize>(bytes: &Bytes, curr_offset: &mut usize) -> [u8; N] {
    unsafe { read_from_bytes(bytes, curr_offset) }
}

/// Read a `T` from `bytes` at the `curr_offset` and advance by `size` bytes.
///
/// SAFETY: Let `value = &bytes[range_move(0..size_of::<T>(), *curr_offset)]`.
/// Then `value` must point to a valid `T` and must be properly aligned for `T`.
unsafe fn read_from_bytes<T: Copy>(bytes: &Bytes, curr_offset: &mut usize) -> T {
    let bytes = &bytes[*curr_offset..];
    *curr_offset += mem::size_of::<T>();
    let ptr: *const T = bytes.as_ptr().cast();
    // SAFETY: Caller promised that `ptr` points to a `T`.
    // Moreover, `ptr` is derived from a shared reference with permission to read this range.
    unsafe { *ptr }
}
