//! Provides the functions [`write_row_to_pages(pages, blob_store, ty, val)`]
//! and [`write_row_to_page(page, blob_store, visitor, ty, val)`]
//! which write `val: ProductValue` typed at `ty` to `page` and `pages` respectively.

use super::{
    blob_store::BlobStore,
    indexes::{Bytes, PageOffset, RowPointer, SquashedOffset},
    layout::{
        align_to, bsatn_len, required_var_len_granules_for_row, AlgebraicTypeLayout, HasLayout, ProductTypeLayout,
        RowTypeLayout, SumTypeLayout, VarLenType,
    },
    page::{GranuleOffsetIter, Page, VarView},
    pages::Pages,
    util::{maybe_uninit_write_slice, range_move},
    var_len::{visit_var_len_assume_init, VarLenGranule, VarLenMembers, VarLenRef},
};
use spacetimedb_sats::{bsatn::to_writer, buffer::BufWriter, AlgebraicType, AlgebraicValue, ProductValue, SumValue};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Expected a value of type {0:?}, but found {1:?}")]
    WrongType(AlgebraicType, AlgebraicValue),
    #[error(transparent)]
    PageError(#[from] super::page::Error),
    #[error(transparent)]
    PagesError(#[from] super::pages::Error),
}

/// Writes `row` typed at `ty` to `pages`
/// using `blob_store` as needed to write large blobs.
///
/// Panics if `val` is not of type `ty`.
///
/// # Safety
///
/// `pages` must be specialized to store rows of `ty`.
/// This includes that its `visitor` must be prepared to visit var-len members within `ty`,
/// and must do so in the same order as a `VarLenVisitorProgram` for `ty` would,
/// i.e. by monotonically increasing offsets.
pub unsafe fn write_row_to_pages(
    pages: &mut Pages,
    visitor: &impl VarLenMembers,
    blob_store: &mut dyn BlobStore,
    ty: &RowTypeLayout,
    val: &ProductValue,
    squashed_offset: SquashedOffset,
) -> Result<RowPointer, Error> {
    let num_granules = required_var_len_granules_for_row(val);

    match pages.with_page_to_insert_row(ty.size(), num_granules, |page| {
        // SAFETY:
        // - Caller promised that `pages` is suitable for storing instances of `ty`
        //   so `page` is also suitable.
        // - Caller promised that `visitor` is prepared to visit for `ty`
        //   and in the same order as a `VarLenVisitorProgram` for `ty` would.
        // - `visitor` came from `pages` which we can trust to visit in the right order.
        unsafe { write_row_to_page(page, blob_store, visitor, ty, val) }
    })? {
        (page, Ok(offset)) => Ok(RowPointer::new(false, page, offset, squashed_offset)),
        (_, Err(e)) => Err(e),
    }
}

/// Writes `row` typed at `ty` to `page`
/// using `blob_store` as needed to write large blobs
/// and `visitor` to fixup var-len pointers in the fixed-len row part.
///
/// Panics if `val` is not of type `ty`.
///
/// # Safety
///
/// - `page` must be prepared to store instances of `ty`.
///
/// - `visitor` must be prepared to visit var-len members within `ty`,
///   and must do so in the same order as a `VarLenVisitorProgram` for `ty` would,
///   i.e. by monotonically increasing offsets.
///
/// - `page` must use a var-len visitor which visits the same var-len members in the same order.
pub unsafe fn write_row_to_page(
    page: &mut Page,
    blob_store: &mut dyn BlobStore,
    visitor: &impl VarLenMembers,
    ty: &RowTypeLayout,
    val: &ProductValue,
) -> Result<PageOffset, Error> {
    let fixed_row_size = ty.size();
    // SAFETY: We've used the right `row_size` and we trust that others have too.
    // `RowTypeLayout` also ensures that we satisfy the minimum row size.
    let fixed_offset = unsafe { page.alloc_fixed_len(fixed_row_size)? };

    // Create the context for writing to `page`.
    let (mut fixed, var_view) = page.split_fixed_var_mut();
    let mut serialized = BflatnSerializedRowBuffer {
        fixed_buf: fixed.get_row_mut(fixed_offset, fixed_row_size),
        curr_offset: 0,
        var_view,
        last_allocated_var_len_index: 0,
        large_blob_insertions: Vec::new(),
    };

    // Write the row to the page. Roll back on any failure.
    if let Err(e) = serialized.write_product(ty.product(), val) {
        // SAFETY: The `visitor` is proper for the row type per caller requirements.
        unsafe { serialized.roll_back_var_len_allocations(visitor) };
        // SAFETY:
        // - `fixed_offset` came from `alloc_fixed_len` so it is in bounds of `page`.
        // - `RowTypeLayout::size()` ensures `fixed_offset` is properly aligned for `FreeCellRef`.
        unsafe { fixed.free(fixed_offset, fixed_row_size) };
        return Err(e);
    }

    // Haven't stored large blobs or init those granules with blob hashes yet, so do it now.
    serialized.write_large_blobs(blob_store);

    Ok(fixed_offset)
}

/// The writing / serialization context used by the function [`write_row_to_page`].
struct BflatnSerializedRowBuffer<'page> {
    /// The work-in-progress fixed part of the row,
    /// allocated inside the page.
    fixed_buf: &'page mut Bytes,

    /// The current offset into `fixed_buf` at which we are writing.
    ///
    /// The various writing methods will advance `curr_offset`.
    curr_offset: usize,

    /// The number of inserted var-len objects to the page.
    last_allocated_var_len_index: usize,

    /// The deferred large-blob insertions
    /// with `Vec<u8>` being the blob bytes to insert to the blob store
    /// and the `VarLenRef` being the destination to write the blob hash.
    large_blob_insertions: Vec<(VarLenRef, Vec<u8>)>,

    /// The mutable view of the variable section of the page.
    var_view: VarView<'page>,
}

impl BflatnSerializedRowBuffer<'_> {
    /// Rolls back all the var-len allocations made when writing the row.
    ///
    /// # Safety
    ///
    /// The `visitor` must be proper for the row type.
    unsafe fn roll_back_var_len_allocations(&mut self, visitor: &impl VarLenMembers) {
        // SAFETY:
        // - `fixed_buf` is properly aligned for the row type
        //    and `fixed_buf.len()` matches exactly the size of the row type.
        // - `fixed_buf`'s `VarLenRef`s are initialized up to `last_allocated_var_len_index`.
        // - `visitor` is proper for the row type.
        let visitor_iter = unsafe { visit_var_len_assume_init(visitor, self.fixed_buf) };
        for vlr in visitor_iter.take(self.last_allocated_var_len_index) {
            // SAFETY: The `vlr` came from the allocation in `write_var_len_obj`
            // which wrote it to the fixed part using `write_var_len_ref`.
            // Thus, it points to a valid `VarLenGranule`.
            unsafe { self.var_view.free_object_ignore_blob(vlr) };
        }
    }

    /// Insert all large blobs into `blob_store` and their hashes to their granules.
    fn write_large_blobs(mut self, blob_store: &mut dyn BlobStore) {
        for (vlr, value) in self.large_blob_insertions {
            // SAFETY: `vlr` was given to us by `alloc_for_slice`
            // so it is properly aligned for a `VarLenGranule` and in bounds of the page.
            // However, as it was added to `self.large_blob_insertion`, it is also uninit.
            unsafe {
                self.var_view.write_large_blob_hash_to_granule(blob_store, &value, vlr);
            }
        }
    }

    /// Write an `val`, an [`AlgebraicValue`], typed at `ty`, to the buffer.
    fn write_value(&mut self, ty: &AlgebraicTypeLayout, val: &AlgebraicValue) -> Result<(), Error> {
        let ty_alignment = ty.align();
        self.curr_offset = align_to(self.curr_offset, ty_alignment);

        match (ty, val) {
            // For sums, select the type based on the sum tag,
            // write the variant data given the variant type,
            // and finally write the tag.
            (AlgebraicTypeLayout::Sum(ty), AlgebraicValue::Sum(val)) => self.write_sum(ty, val)?,
            // For products, write every element in order.
            (AlgebraicTypeLayout::Product(ty), AlgebraicValue::Product(val)) => self.write_product(ty, val)?,

            // For primitive types, write their contents by LE-encoding.
            (&AlgebraicTypeLayout::Bool, AlgebraicValue::Bool(val)) => self.write_bool(*val),
            // Integer types:
            (&AlgebraicTypeLayout::I8, AlgebraicValue::I8(val)) => self.write_i8(*val),
            (&AlgebraicTypeLayout::U8, AlgebraicValue::U8(val)) => self.write_u8(*val),
            (&AlgebraicTypeLayout::I16, AlgebraicValue::I16(val)) => self.write_i16(*val),
            (&AlgebraicTypeLayout::U16, AlgebraicValue::U16(val)) => self.write_u16(*val),
            (&AlgebraicTypeLayout::I32, AlgebraicValue::I32(val)) => self.write_i32(*val),
            (&AlgebraicTypeLayout::U32, AlgebraicValue::U32(val)) => self.write_u32(*val),
            (&AlgebraicTypeLayout::I64, AlgebraicValue::I64(val)) => self.write_i64(*val),
            (&AlgebraicTypeLayout::U64, AlgebraicValue::U64(val)) => self.write_u64(*val),
            (&AlgebraicTypeLayout::I128, AlgebraicValue::I128(val)) => self.write_i128(*val),
            (&AlgebraicTypeLayout::U128, AlgebraicValue::U128(val)) => self.write_u128(*val),
            // Float types:
            (&AlgebraicTypeLayout::F32, AlgebraicValue::F32(val)) => self.write_f32((*val).into()),
            (&AlgebraicTypeLayout::F64, AlgebraicValue::F64(val)) => self.write_f64((*val).into()),

            // For strings, we reserve space for a `VarLenRef`
            // and push the bytes as a var-len object.
            (&AlgebraicTypeLayout::String, AlgebraicValue::String(val)) => self.write_string(val)?,

            // For array and maps, we reserve space for a `VarLenRef`
            // and push the bytes, after BSATN encoding, as a var-len object.
            (AlgebraicTypeLayout::VarLen(VarLenType::Array(_)), val @ AlgebraicValue::Array(_))
            | (AlgebraicTypeLayout::VarLen(VarLenType::Map(_)), val @ AlgebraicValue::Map(_)) => {
                self.write_av_bsatn(val)?
            }

            // If the type doesn't match the value, return an error.
            (ty, val) => Err(Error::WrongType(ty.algebraic_type(), val.clone()))?,
        }

        self.curr_offset = align_to(self.curr_offset, ty_alignment);

        Ok(())
    }

    /// Write a `val`, a [`SumValue`], typed at `ty`, to the buffer.
    fn write_sum(&mut self, ty: &SumTypeLayout, val: &SumValue) -> Result<(), Error> {
        // Extract sum value components and variant type, and offsets.
        let SumValue { tag, ref value } = *val;
        let variant_ty = &ty.variants[tag as usize];
        let variant_offset = self.curr_offset + ty.offset_of_variant_data(tag);
        let tag_offset = self.curr_offset + ty.offset_of_tag();

        // Write the variant value at `variant_offset`.
        self.curr_offset = variant_offset;
        self.write_value(&variant_ty.ty, value)?;

        // Write the variant value at `tag_offset`.
        self.curr_offset = tag_offset;
        self.write_u8(tag);

        Ok(())
    }

    /// Write an `val`, a [`ProductValue`], typed at `ty`, to the buffer.
    fn write_product(&mut self, ty: &ProductTypeLayout, val: &ProductValue) -> Result<(), Error> {
        // `Iterator::zip` silently drops elements if the two iterators have different lengths,
        // so we need to check that our `ProductValue` has the same number of elements
        // as our `ProductTypeLayout` to be sure it's typed correctly.
        // Otherwise, if the value is too long, we'll discard its fields (whatever),
        // or if it's too long, we'll leave some fields in the page uninit (very bad).
        if ty.elements.len() != val.elements.len() {
            return Err(Error::WrongType(
                ty.algebraic_type(),
                AlgebraicValue::Product(val.clone()),
            ));
        }
        for (elt_ty, elt) in ty.elements.iter().zip(val.elements.iter()) {
            self.write_value(&elt_ty.ty, elt)?;
        }
        Ok(())
    }

    /// Write the string `str` to the var-len section
    /// and a `VarLenRef` to the fixed buffer and advance the `curr_offset`.
    fn write_string(&mut self, val: &str) -> Result<(), Error> {
        let val = val.as_bytes();

        // Write `val` to the page. The handle is `vlr`.
        let (vlr, in_blob) = self.var_view.alloc_for_slice(val)?;
        if in_blob {
            self.defer_insert_large_blob(vlr, val.to_vec());
        }

        // Write `vlr` to the fixed part.
        self.write_var_len_ref(vlr);
        Ok(())
    }

    /// Write `val` BSATN-encoded to var-len section
    /// and a `VarLenRef` to the fixed buffer and advance the `curr_offset`.
    fn write_av_bsatn(&mut self, val: &AlgebraicValue) -> Result<(), Error> {
        // Allocate space.
        let len_in_bytes = bsatn_len(val);
        let (vlr, in_blob) = self.var_view.alloc_for_len(len_in_bytes)?;

        // Write `vlr` to the fixed part.
        self.write_var_len_ref(vlr);

        if in_blob {
            // We won't be storing the large blob in the page,
            // so no point in writing the blob directly to the page.
            let mut bytes = Vec::with_capacity(len_in_bytes);
            val.encode(&mut bytes);
            self.defer_insert_large_blob(vlr, bytes);
        } else {
            // Write directly to the page.
            // SAFETY: `vlr.first_granule` points to a granule
            // even though the granule's data is uninit as of yet.
            let iter = unsafe { self.var_view.granule_offset_iter(vlr.first_granule) };
            let mut writer = GranuleBufWriter { buf: None, iter };
            to_writer(&mut writer, val).unwrap();
        }

        /// A `BufWriter` that writes directly to a page.
        struct GranuleBufWriter<'vv, 'page> {
            /// The offset to the granule being written to
            /// and how much has been written to it already.
            buf: Option<(PageOffset, usize)>,
            /// The iterator for the offsets to all the granule we'll write to.
            iter: GranuleOffsetIter<'page, 'vv>,
        }
        impl BufWriter for GranuleBufWriter<'_, '_> {
            fn put_slice(&mut self, mut slice: &[u8]) {
                while !slice.is_empty() {
                    let (offset, start) = match self.buf.take() {
                        // Still have some to write to this granule.
                        Some(buf @ (_, start)) if start < VarLenGranule::DATA_SIZE => buf,
                        // First granule or the current one is full.
                        _ => {
                            let next = self.iter.next();
                            debug_assert!(next.is_some());
                            // SAFETY: The iterator length is exactly such that
                            // `next.is_none() == slice.is_empty()`.
                            let next = unsafe { next.unwrap_unchecked() };
                            (next, 0)
                        }
                    };

                    // Derive how much we can add to this granule
                    // and only take that much from `slice`.
                    let capacity_remains = VarLenGranule::DATA_SIZE - start;
                    debug_assert!(capacity_remains > 0);
                    let extend_len = capacity_remains.min(slice.len());
                    let (extend_with, rest) = slice.split_at(extend_len);
                    // The section of the granule data to write to.
                    // SAFETY:
                    // - `offset` came from `self.iter`, which only yields valid offsets.
                    // - `start < VarLenGranule::DATA_SIZE` was ensured above.
                    let write_to = unsafe { self.iter.get_mut_data(offset, start) };

                    // Write to the granule.
                    for (to, byte) in write_to.iter_mut().zip(extend_with) {
                        to.write(*byte);
                    }

                    slice = rest;
                    self.buf = Some((offset, start + extend_len));
                }
            }
        }

        Ok(())
    }

    /// Write a `VarLenRef` to the fixed buffer and advance the `curr_offset`.
    fn write_var_len_ref(&mut self, val: VarLenRef) {
        self.write_u16(val.length_in_bytes);
        self.write_u16(val.first_granule.0);

        // Keep track of how many var len objects we've added so far
        // so that we can free them on failure.
        self.last_allocated_var_len_index += 1;
    }

    /// Defers the insertion of a large blob to the blob store as well as writing the hash to the granule.
    fn defer_insert_large_blob(&mut self, vlr: VarLenRef, obj_bytes: Vec<u8>) {
        self.large_blob_insertions.push((vlr, obj_bytes));
    }

    /// Write `bytes: &[u8; N]` starting at the current offset
    /// and advance the offset by `N`.
    fn write_bytes<const N: usize>(&mut self, bytes: &[u8; N]) {
        maybe_uninit_write_slice(&mut self.fixed_buf[range_move(0..N, self.curr_offset)], bytes);
        self.curr_offset += N;
    }

    /// Write a `u8` to the fixed buffer and advance the `curr_offset`.
    fn write_u8(&mut self, val: u8) {
        self.write_bytes(&[val]);
    }

    /// Write an `i8` to the fixed buffer and advance the `curr_offset`.
    fn write_i8(&mut self, val: i8) {
        self.write_u8(val as u8);
    }

    /// Write a `bool` to the fixed buffer and advance the `curr_offset`.
    fn write_bool(&mut self, val: bool) {
        self.write_u8(val as u8);
    }

    /// Write a `u16` to the fixed buffer and advance the `curr_offset`.
    fn write_u16(&mut self, val: u16) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write an `i16` to the fixed buffer and advance the `curr_offset`.
    fn write_i16(&mut self, val: i16) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write a `u32` to the fixed buffer and advance the `curr_offset`.
    fn write_u32(&mut self, val: u32) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write an `i32` to the fixed buffer and advance the `curr_offset`.
    fn write_i32(&mut self, val: i32) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write a `u64` to the fixed buffer and advance the `curr_offset`.
    fn write_u64(&mut self, val: u64) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write an `i64` to the fixed buffer and advance the `curr_offset`.
    fn write_i64(&mut self, val: i64) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write a `u128` to the fixed buffer and advance the `curr_offset`.
    fn write_u128(&mut self, val: u128) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write an `i128` to the fixed buffer and advance the `curr_offset`.
    fn write_i128(&mut self, val: i128) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write a `f32` to the fixed buffer and advance the `curr_offset`.
    fn write_f32(&mut self, val: f32) {
        self.write_bytes(&val.to_le_bytes());
    }

    /// Write a `f64` to the fixed buffer and advance the `curr_offset`.
    fn write_f64(&mut self, val: f64) {
        self.write_bytes(&val.to_le_bytes());
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::proptest_sats::generate_typed_row;
    use crate::{
        bflatn_from::serialize_row_from_page, blob_store::HashMapBlobStore, row_type_visitor::row_type_visitor,
    };
    use proptest::{prelude::*, prop_assert_eq, proptest};
    use spacetimedb_sats::algebraic_value::ser::ValueSerializer;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2048))]
        #[test]
        fn av_serde_round_trip_through_page((ty, val) in generate_typed_row()) {
            let ty: RowTypeLayout = ty.into();
            let mut page = Page::new(ty.size());
            let visitor = row_type_visitor(&ty);
            let blob_store = &mut HashMapBlobStore::default();

            let offset = unsafe { write_row_to_page(&mut page, blob_store, &visitor, &ty, &val).unwrap() };

            let read_val = unsafe { serialize_row_from_page(ValueSerializer, &page, blob_store, offset, &ty) }
                .unwrap().into_product().unwrap();

            prop_assert_eq!(val, read_val);
        }
    }
}
