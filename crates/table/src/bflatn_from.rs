//! Provides the function [`read_row_from_page(ser, page, fixed_offset, ty)`]
//! which serializes `value = page.get_row_data(fixed_offset, fixed_row_size)` typed at `ty`
//! and associated var len objects in `value` into the serializer `ser`.

use crate::layout::ProductTypeLayoutView;

use super::{
    blob_store::BlobStore,
    indexes::{Bytes, PageOffset},
    layout::{align_to, AlgebraicTypeLayout, HasLayout as _, RowTypeLayout, SumTypeLayout, VarLenType},
    page::Page,
    row_hash,
    var_len::VarLenRef,
};
use core::cell::Cell;
use core::str;
use spacetimedb_sats::{
    i256, impl_serialize,
    ser::{SerializeNamedProduct, Serializer},
    u256, AlgebraicType,
};

/// Serializes the row in `page` where the fixed part starts at `fixed_offset`
/// and lasts `ty.size()` bytes. This region is typed at `ty`.
///
/// # Safety
///
/// 1. the `fixed_offset` must point at a row in `page` lasting `ty.size()` byte.
/// 2. the row must be a valid `ty`.
/// 3. for any `vlr: VarLenRef` stored in the row,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
pub unsafe fn serialize_row_from_page<S: Serializer>(
    ser: S,
    page: &Page,
    blob_store: &dyn BlobStore,
    fixed_offset: PageOffset,
    ty: &RowTypeLayout,
) -> Result<S::Ok, S::Error> {
    let fixed_bytes = page.get_row_data(fixed_offset, ty.size());
    // SAFETY:
    // - Per 1. and 2., `fixed_bytes` points at a row in `page` valid for `ty`.
    // - Per 3., for any `vlr: VarLenRef` stored in `fixed_bytes`,
    //   `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
    unsafe { serialize_product(ser, fixed_bytes, page, blob_store, &Cell::new(0), ty.product()) }
}

/// This has to be a `Cell<_>` here as we only get `&Value` in `Serialize`.
type CurrOffset<'a> = &'a Cell<usize>;

/// Updates `curr_offset` by running `with` on a copy of its current value.
fn update<R>(curr_offset: CurrOffset<'_>, with: impl FnOnce(&mut usize) -> R) -> R {
    let mut tmp = curr_offset.get();
    let ret = with(&mut tmp);
    curr_offset.set(tmp);
    ret
}

/// Serializes every product field in `value = &bytes[range_move(0..ty.size(), *curr_offset)]`,
/// which is typed at `ty`, into `ser`.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn serialize_product<S: Serializer>(
    ser: S,
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: CurrOffset<'_>,
    ty: ProductTypeLayoutView<'_>,
) -> Result<S::Ok, S::Error> {
    let elems = &ty.elements;
    let mut ser = ser.serialize_named_product(elems.len())?;

    let my_offset = curr_offset.get();

    for elem_ty in elems.iter() {
        curr_offset.set(my_offset + elem_ty.offset as usize);
        // SAFETY: By 1., `value` is valid at `ty`,
        // so it follows that valid and properly aligned sub-`value`s
        // are valid `elem_ty.ty`s.
        // By 2., and the above, it follows that sub-`value`s won't have dangling `VarLenRef`s.
        let value = Value {
            bytes,
            page,
            blob_store,
            curr_offset,
            ty: &elem_ty.ty,
        };
        ser.serialize_element(elem_ty.name.as_deref(), &value)?;
    }

    ser.end()
}

/// Serializes the sum value in `value = &bytes[range_move(0..ty.size(), *curr_offset)]`,
/// which is typed at `ty`, into `ser`.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn serialize_sum<S: Serializer>(
    ser: S,
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: CurrOffset<'_>,
    ty: &SumTypeLayout,
) -> Result<S::Ok, S::Error> {
    // Read the tag of the sum value.
    let (tag, data_ty) = read_tag(bytes, ty, curr_offset.get());

    // Serialize the variant data value.
    let data_offset = &Cell::new(curr_offset.get() + ty.offset_of_variant_data(tag));
    // SAFETY: `value` is valid at `ty` so given `tag`,
    // we know `data_value = &bytes[range_move(0..data_ty.size(), data_offset))`
    // is valid at `data_ty`.
    // By 2., and the above, we also know that `data_value` won't have dangling `VarLenRef`s.
    let data_value = Value {
        bytes,
        page,
        blob_store,
        curr_offset: data_offset,
        ty: data_ty,
    };
    let ret = ser.serialize_variant(tag, None, &data_value);

    update(curr_offset, |co| *co += ty.size());
    ret
}

/// Reads the tag of the sum value and selects the data variant type.
pub fn read_tag<'ty>(bytes: &Bytes, ty: &'ty SumTypeLayout, curr_offset: usize) -> (u8, &'ty AlgebraicTypeLayout) {
    let tag_offset = ty.offset_of_tag();
    let tag = bytes[curr_offset + tag_offset];

    // Extract the variant data type depending on the tag.
    let data_ty = &ty.variants[tag as usize].ty;

    (tag, data_ty)
}

/// A `Serialize` version of `serialize_value`.
///
/// SAFETY: Constructing a value of this type
/// has the same safety requirements as calling `serialize_value`.
struct Value<'a> {
    bytes: &'a Bytes,
    page: &'a Page,
    blob_store: &'a dyn BlobStore,
    curr_offset: CurrOffset<'a>,
    ty: &'a AlgebraicTypeLayout,
}

impl_serialize!(['a] Value<'a>, (self, ser) => {
    unsafe { serialize_value(ser, self.bytes, self.page, self.blob_store, self.curr_offset, self.ty) }
});

/// Serialize `value = &bytes[range_move(0..ty.size(), *curr_offset)]` into a `ser`,
/// using `blob_store` to retrieve the bytes of any large blob object
/// and `curr_offset`, advanced as serialization progresses, to decide where to start reading.
///
/// SAFETY:
/// 1. the `value` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
/// 3. `align_to(curr_offset.get(), ty.align())` must be the offset of a field typed at `ty`.
pub(crate) unsafe fn serialize_value<S: Serializer>(
    ser: S,
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: CurrOffset<'_>,
    ty: &AlgebraicTypeLayout,
) -> Result<S::Ok, S::Error> {
    debug_assert_eq!(
        curr_offset.get(),
        align_to(curr_offset.get(), ty.align()),
        "curr_offset {curr_offset:?} insufficiently aligned for type {ty:#?}",
    );

    match ty {
        AlgebraicTypeLayout::Sum(ty) => {
            // SAFETY: `value` was valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { serialize_sum(ser, bytes, page, blob_store, curr_offset, ty) }
        }
        AlgebraicTypeLayout::Product(ty) => {
            // SAFETY: `value` was valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { serialize_product(ser, bytes, page, blob_store, curr_offset, ty.view()) }
        }
        // The primitive types:
        //
        // SAFETY (applies to app primitive types):
        // Per caller requirement, know `value` points to a valid `ty`.
        // Thus `&bytes[range_move(0..ty.size(), *curr_offset)]` points to init bytes
        // and `ty.size()` corresponds exactly to `N = 1, 1, 1, 2, 2, 4, 4, 8, 8, 16, 16, 32, 32, 4, 8`.
        &AlgebraicTypeLayout::Bool => ser.serialize_bool(unsafe { read_from_bytes::<u8>(bytes, curr_offset) } != 0),
        &AlgebraicTypeLayout::I8 => ser.serialize_i8(unsafe { read_from_bytes(bytes, curr_offset) }),
        &AlgebraicTypeLayout::U8 => ser.serialize_u8(unsafe { read_from_bytes(bytes, curr_offset) }),
        &AlgebraicTypeLayout::I16 => {
            ser.serialize_i16(i16::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U16 => {
            ser.serialize_u16(u16::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::I32 => {
            ser.serialize_i32(i32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U32 => {
            ser.serialize_u32(u32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::I64 => {
            ser.serialize_i64(i64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U64 => {
            ser.serialize_u64(u64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::I128 => {
            ser.serialize_i128(i128::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U128 => {
            ser.serialize_u128(u128::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::I256 => {
            ser.serialize_i256(i256::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::U256 => {
            ser.serialize_u256(u256::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::F32 => {
            ser.serialize_f32(f32::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }
        &AlgebraicTypeLayout::F64 => {
            ser.serialize_f64(f64::from_le_bytes(unsafe { read_from_bytes(bytes, curr_offset) }))
        }

        // The var-len cases.
        &AlgebraicTypeLayout::String => {
            // SAFETY: `value` was valid at `::String` and `VarLenRef`s won't be dangling.
            unsafe { serialize_string(ser, bytes, page, blob_store, curr_offset) }
        }
        AlgebraicTypeLayout::VarLen(VarLenType::Array(ty)) => {
            // SAFETY: `value` was valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { serialize_bsatn(ser, bytes, page, blob_store, curr_offset, ty) }
        }
    }
}

/// Serialize `value = &bytes[range_move(0..::String.size(), *curr_offset)]` into a `ser`,
/// using `blob_store` to retrieve the bytes of any large blob object
/// and `curr_offset`, advanced as serialization progresses, to decide where to start reading.
///
/// SAFETY:
/// 1. the `value` must be valid at type `::String` and properly aligned for `::String``.
/// 2. for any `vlr: VarLenRef` stored in `value`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
unsafe fn serialize_string<S: Serializer>(
    ser: S,
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: CurrOffset<'_>,
) -> Result<S::Ok, S::Error> {
    // SAFETY: `value` was valid at and aligned for `::String`
    // which stores a `vlr: VarLenRef` as its fixed value.
    // The range thus is valid and properly aligned for `VarLenRef`.
    let vlr = unsafe { read_from_bytes::<VarLenRef>(bytes, curr_offset) };

    if vlr.is_large_blob() {
        // SAFETY: As `vlr` a blob, `vlr.first_granule` always points to a valid granule.
        let blob = unsafe { vlr_blob_bytes(page, blob_store, vlr) };
        // SAFETY: For `::String`, the blob will always be valid UTF-8.
        let str = unsafe { str::from_utf8_unchecked(blob) };
        ser.serialize_str(str)
    } else {
        // SAFETY: `vlr.first_granule` is either NULL or points to a valid granule.
        let var_iter = unsafe { page.iter_vlo_data(vlr.first_granule) };
        let total_len = vlr.length_in_bytes as usize;
        // SAFETY:
        // - `total_len <= isize::MAX` is the total length of the granules concatenated.
        // - The aforementioned concatenation is valid UTF-8.
        unsafe { ser.serialize_str_in_chunks(total_len, var_iter) }
    }
}

unsafe fn serialize_bsatn<S: Serializer>(
    ser: S,
    bytes: &Bytes,
    page: &Page,
    blob_store: &dyn BlobStore,
    curr_offset: CurrOffset<'_>,
    ty: &AlgebraicType,
) -> Result<S::Ok, S::Error> {
    // SAFETY: `value` was valid at and aligned for `ty`.
    // These `ty` store a `vlr: VarLenRef` as their fixed value.
    // The range thus is valid and properly aligned for `VarLenRef`.
    let vlr = unsafe { read_from_bytes::<VarLenRef>(bytes, curr_offset) };

    if vlr.is_large_blob() {
        // SAFETY: As `vlr` is a blob, `vlr.first_granule` always points to a valid granule.
        let blob = unsafe { vlr_blob_bytes(page, blob_store, vlr) };
        // SAFETY: The BSATN in `blob` is encoded from an `AlgebraicValue`.
        unsafe { ser.serialize_bsatn(ty, blob) }
    } else {
        // SAFETY: `vlr.first_granule` is either NULL or points to a valid granule.
        let var_iter = unsafe { page.iter_vlo_data(vlr.first_granule) };
        let total_len = vlr.length_in_bytes as usize;
        // SAFETY:
        // - `total_len <= isize::MAX` is the total length of the granules concatenated.
        // - The BSATN in `blob` is encoded from an `AlgebraicValue`.
        unsafe { ser.serialize_bsatn_in_chunks(ty, total_len, var_iter) }
    }
}

/// Get the large blob object that `vlr.first_granule` points to.
///
/// SAFETY:
/// - `vlr.first_granule` must point to a valid granule in `page`.
#[cold]
#[inline(never)]
pub(crate) unsafe fn vlr_blob_bytes<'b>(page: &Page, blob_store: &'b dyn BlobStore, vlr: VarLenRef) -> &'b [u8] {
    // Read the blob hash.
    // SAFETY: `vlr.first_granule` points to a valid granule.
    let mut var_iter = unsafe { page.iter_var_len_object(vlr.first_granule) };
    let granule = var_iter.next();
    // SAFETY: As it pointed to a valid granule and not null,
    // the iterator will never yield `None` on the first call.
    let granule = unsafe { granule.unwrap_unchecked() };
    let hash = granule.blob_hash();

    // Find the blob.
    blob_store.retrieve_blob(&hash).unwrap()
}

/// Read a `T` from `bytes` at the `curr_offset` and advance by `size` bytes.
///
/// # Safety
///
/// Let `value = &bytes[range_move(0..size_of::<T>(), *curr_offset)]`.
/// Then `value` must point to a valid `T` and must be properly aligned for `T`.
pub unsafe fn read_from_bytes<T: Copy>(bytes: &Bytes, curr_offset: CurrOffset<'_>) -> T {
    // SAFETY: forward caller requirements.
    update(curr_offset, |co| unsafe { row_hash::read_from_bytes(bytes, co) })
}
