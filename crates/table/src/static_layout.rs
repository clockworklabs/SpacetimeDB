//! This module implements a fast path for converting certain row types between BFLATN <-> BSATN.
//!
//! The key insight is that a majority of row types will have a known fixed length,
//! with no variable-length members.
//! BFLATN is designed with this in mind, storing fixed-length portions of rows inline,
//! at the expense of an indirection to reach var-length columns like strings.
//! A majority of these types will also have a fixed BSATN length,
//! but note that BSATN stores sum values (enums) without padding,
//! so row types which contain sums may not have a fixed BSATN length
//! if the sum's variants have different "live" unpadded lengths.
//!
//! For row types with fixed BSATN lengths, we can reduce the BFLATN <-> BSATN conversions
//! to a series of `memcpy`s, skipping over padding sequences.
//! This is potentially much faster than the more general
//! [`crate::bflatn_from::serialize_row_from_page`] and [`crate::bflatn_to::write_row_to_page`] ,
//! which both traverse a [`RowTypeLayout`] and dispatch on the type of each column.
//!
//! For example, to serialize a row of type `(u64, u64, u32, u64)`,
//! [`bflatn_from`] will do four dispatches, three calls to `serialize_u64` and one to `serialize_u32`.
//! This module will make 2 `memcpy`s (or actually, `<[u8]>::copy_from_slice`s):
//! one of 20 bytes to copy the leading `(u64, u64, u32)`, which contains no padding,
//! and then one of 8 bytes to copy the trailing `u64`, skipping over 4 bytes of padding in between.

use smallvec::SmallVec;
use spacetimedb_data_structures::slim_slice::SlimSmallSliceBox;

use crate::layout::ProductTypeLayoutView;

use super::{
    indexes::{Byte, Bytes},
    layout::{
        AlgebraicTypeLayout, HasLayout, PrimitiveType, ProductTypeElementLayout, RowTypeLayout, SumTypeLayout,
        SumTypeVariantLayout,
    },
    util::range_move,
    MemoryUsage,
};
use core::mem::MaybeUninit;
use core::ptr;

/// A precomputed layout for a type whose encoded BSATN and BFLATN lengths are both known constants,
/// enabling fast BFLATN <-> BSATN conversions.
#[derive(PartialEq, Eq, Debug, Clone)]
#[repr(align(8))]
pub struct StaticLayout {
    /// The length of the encoded BSATN representation of a row of this type,
    /// in bytes.
    ///
    /// Storing this allows us to pre-allocate correctly-sized buffers,
    /// avoiding potentially-expensive `realloc`s.
    pub(crate) bsatn_length: u16,

    /// A series of `memcpy` invocations from a BFLATN src/dst <-> a BSATN src/dst
    /// which are sufficient to convert BSATN to BFLATN and vice versa.
    fields: SlimSmallSliceBox<MemcpyField, 3>,
}

impl MemoryUsage for StaticLayout {
    fn heap_usage(&self) -> usize {
        let Self { bsatn_length, fields } = self;
        bsatn_length.heap_usage() + fields.heap_usage()
    }
}

impl StaticLayout {
    /// Serialize `row` from BFLATN to BSATN into `buf`.
    ///
    /// # Safety
    ///
    /// - `buf` must be at least `self.bsatn_length` long.
    /// - `row` must store a valid, initialized instance of the BFLATN row type
    ///   for which `self` was computed.
    ///   As a consequence of this, for every `field` in `self.fields`,
    ///   `row[field.bflatn_offset .. field.bflatn_offset + length]` will be initialized.
    unsafe fn serialize_row_into(&self, buf: &mut [MaybeUninit<Byte>], row: &Bytes) {
        debug_assert!(buf.len() >= self.bsatn_length as usize);
        for field in &*self.fields {
            // SAFETY: forward caller requirements.
            unsafe { field.copy_bflatn_to_bsatn(row, buf) };
        }
    }

    /// Serialize `row` from BFLATN to BSATN into a `Vec<u8>`.
    ///
    /// # Safety
    ///
    /// - `row` must store a valid, initialized instance of the BFLATN row type
    ///   for which `self` was computed.
    ///   As a consequence of this, for every `field` in `self.fields`,
    ///   `row[field.bflatn_offset .. field.bflatn_offset + length]` will be initialized.
    pub(crate) unsafe fn serialize_row_into_vec(&self, row: &Bytes) -> Vec<u8> {
        // Create an uninitialized buffer `buf` of the correct length.
        let bsatn_len = self.bsatn_length as usize;
        let mut buf = Vec::with_capacity(bsatn_len);
        let sink = buf.spare_capacity_mut();

        // (1) Write the row into the slice using a series of `memcpy`s.
        // SAFETY:
        // - Caller promised that `row` is valid for `self`.
        // - `sink` was constructed with exactly the correct length above.
        unsafe {
            self.serialize_row_into(sink, row);
        }

        // SAFETY: In (1), we initialized `0..len`
        // as `row` was valid for `self` per caller requirements.
        unsafe { buf.set_len(bsatn_len) }
        buf
    }

    /// Serialize `row` from BFLATN to BSATN, appending the BSATN to `buf`.
    ///
    /// # Safety
    ///
    /// - `row` must store a valid, initialized instance of the BFLATN row type
    ///   for which `self` was computed.
    ///   As a consequence of this, for every `field` in `self.fields`,
    ///   `row[field.bflatn_offset .. field.bflatn_offset + length]` will be initialized.
    pub(crate) unsafe fn serialize_row_extend(&self, buf: &mut Vec<u8>, row: &Bytes) {
        // Get an uninitialized slice within `buf` of the correct length.
        let start = buf.len();
        let len = self.bsatn_length as usize;
        buf.reserve(len);
        let sink = &mut buf.spare_capacity_mut()[..len];

        // (1) Write the row into the slice using a series of `memcpy`s.
        // SAFETY:
        // - Caller promised that `row` is valid for `self`.
        // - `sink` was constructed with exactly the correct length above.
        unsafe {
            self.serialize_row_into(sink, row);
        }

        // SAFETY: In (1), we initialized `start .. start + len`
        // as `row` was valid for `self` per caller requirements
        // and we had initialized up to `start` before,
        // so now we have initialized up to `start + len`.
        unsafe { buf.set_len(start + len) }
    }

    #[allow(unused)]
    /// Deserializes the BSATN-encoded `row` into the BFLATN-encoded `buf`.
    ///
    /// - `row` must be at least `self.bsatn_length` long.
    /// - `buf` must be ready to store an instance of the BFLATN row type
    ///   for which `self` was computed.
    ///   As a consequence of this, for every `field` in `self.fields`,
    ///   `field.bflatn_offset .. field.bflatn_offset + length` must be in-bounds of `buf`.
    pub(crate) unsafe fn deserialize_row_into(&self, buf: &mut Bytes, row: &[u8]) {
        for field in &*self.fields {
            // SAFETY: forward caller requirements.
            unsafe { field.copy_bsatn_to_bflatn(row, buf) };
        }
    }

    /// Compares `row_a` for equality against `row_b`.
    ///
    /// # Safety
    ///
    /// - `row` must store a valid, initialized instance of the BFLATN row type
    ///   for which `self` was computed.
    ///   As a consequence of this, for every `field` in `self.fields`,
    ///   `row[field.bflatn_offset .. field.bflatn_offset + field.length]` will be initialized.
    pub(crate) unsafe fn eq(&self, row_a: &Bytes, row_b: &Bytes) -> bool {
        // No need to check the lengths.
        // We assume they are of the same length.
        self.fields.iter().all(|field| {
            // SAFETY: The consequence of what the caller promised is that
            // `row_(a/b).len() >= field.bflatn_offset + field.length >= field.bflatn_offset`.
            unsafe { field.eq(row_a, row_b) }
        })
    }

    /// Construct a `StaticLayout` for converting BFLATN rows of `row_type` <-> BSATN.
    ///
    /// Returns `None` if `row_type` contains a column which does not have a constant length in BSATN,
    /// either a [`VarLenType`]
    /// or a [`SumTypeLayout`] whose variants do not have the same "live" unpadded length.
    pub fn for_row_type(row_type: &RowTypeLayout) -> Option<Self> {
        if !row_type.layout().fixed {
            // Don't bother computing the static layout if there are variable components.
            return None;
        }

        let mut builder = LayoutBuilder::new_builder();
        builder.visit_product(row_type.product())?;
        Some(builder.build())
    }
}

/// An identifier for a series of bytes within a BFLATN row
/// which can be directly copied into an output BSATN buffer
/// with a known length and offset or vice versa.
///
/// Within the row type's BFLATN layout, `row[bflatn_offset .. (bflatn_offset + length)]`
/// must not contain any padding bytes,
/// i.e. all of those bytes must be fully initialized if the row is initialized.
#[derive(PartialEq, Eq, Debug, Copy, Clone)]
struct MemcpyField {
    /// Offset in the BFLATN row from which to begin `memcpy`ing, in bytes.
    bflatn_offset: u16,

    /// Offset in the BSATN buffer to which to begin `memcpy`ing, in bytes.
    // TODO(perf): Could be a running counter, but this way we just have all the `memcpy` args in one place.
    // Should bench; I (pgoldman 2024-03-25) suspect this allows more insn parallelism and is therefore better.
    bsatn_offset: u16,

    /// Length to `memcpy`, in bytes.
    length: u16,
}

impl MemoryUsage for MemcpyField {}

impl MemcpyField {
    /// Copies the bytes at `src[self.bflatn_offset .. self.bflatn_offset + self.length]`
    /// into `dst[self.bsatn_offset .. self.bsatn_offset + self.length]`.
    ///
    /// # Safety
    ///
    /// 1. `src.len() >= self.bflatn_offset + self.length`.
    /// 2. `dst.len() >= self.bsatn_offset + self.length`
    unsafe fn copy_bflatn_to_bsatn(&self, src: &Bytes, dst: &mut [MaybeUninit<Byte>]) {
        let src_offset = self.bflatn_offset as usize;
        let dst_offset = self.bsatn_offset as usize;

        let len = self.length as usize;
        let src = src.as_ptr();
        let dst = dst.as_mut_ptr();
        // SAFETY: per 1., it follows that `src_offset` is in bounds of `src`.
        let src = unsafe { src.add(src_offset) };
        // SAFETY: per 2., it follows that `dst_offset` is in bounds of `dst`.
        let dst = unsafe { dst.add(dst_offset) };
        let dst = dst.cast();

        // SAFETY:
        // 1. `src` is valid for reads for `len` bytes per caller requirement 1.
        //    and because `src` was derived from a shared slice.
        // 2. `dst` is valid for writes for `len` bytes per caller requirement 2.
        //    and because `dst` was derived from an exclusive slice.
        // 3. Alignment for `u8` is trivially satisfied for any pointer.
        // 4. As `src` and `dst` were derived from shared and exclusive slices, they cannot overlap.
        unsafe { ptr::copy_nonoverlapping(src, dst, len) }
    }

    /// Copies the bytes at `src[self.bsatn_offset .. self.bsatn_offset + self.length]`
    /// into `dst[self.bflatn_offset .. self.bflatn_offset + self.length]`.
    ///
    /// # Safety
    ///
    /// 1. `src.len() >= self.bsatn_offset + self.length`.
    /// 2. `dst.len() >= self.bflatn_offset + self.length`
    unsafe fn copy_bsatn_to_bflatn(&self, src: &Bytes, dst: &mut Bytes) {
        let src_offset = self.bsatn_offset as usize;
        let dst_offset = self.bflatn_offset as usize;

        let len = self.length as usize;
        let src = src.as_ptr();
        let dst = dst.as_mut_ptr();
        // SAFETY: per 1., it follows that `src_offset` is in bounds of `src`.
        let src = unsafe { src.add(src_offset) };
        // SAFETY: per 2., it follows that `dst_offset` is in bounds of `dst`.
        let dst = unsafe { dst.add(dst_offset) };

        // SAFETY:
        // 1. `src` is valid for reads for `len` bytes per caller requirement 1.
        //    and because `src` was derived from a shared slice.
        // 2. `dst` is valid for writes for `len` bytes per caller requirement 2.
        //    and because `dst` was derived from an exclusive slice.
        // 3. Alignment for `u8` is trivially satisfied for any pointer.
        // 4. As `src` and `dst` were derived from shared and exclusive slices, they cannot overlap.
        unsafe { ptr::copy_nonoverlapping(src, dst, len) }
    }

    /// Compares `row_a` and `row_b` for equality in this field.
    ///
    /// # Safety
    ///
    /// - `row_a.len() >= self.bflatn_offset + self.length`
    /// - `row_b.len() >= self.bflatn_offset + self.length`
    unsafe fn eq(&self, row_a: &Bytes, row_b: &Bytes) -> bool {
        let range = range_move(0..self.length as usize, self.bflatn_offset as usize);
        let range2 = range.clone();
        // SAFETY: The `range` is in bounds as
        // `row_a.len() >= self.bflatn_offset + self.length >= self.bflatn_offset`.
        let row_a_field = unsafe { row_a.get_unchecked(range) };
        // SAFETY: The `range` is in bounds as
        // `row_b.len() >= self.bflatn_offset + self.length >= self.bflatn_offset`.
        let row_b_field = unsafe { row_b.get_unchecked(range2) };
        row_a_field == row_b_field
    }

    fn is_empty(&self) -> bool {
        self.length == 0
    }
}

/// A builder for a [`StaticLayout`].
struct LayoutBuilder {
    /// Always at least one element.
    fields: Vec<MemcpyField>,
}

impl LayoutBuilder {
    fn new_builder() -> Self {
        Self {
            fields: vec![MemcpyField {
                bflatn_offset: 0,
                bsatn_offset: 0,
                length: 0,
            }],
        }
    }

    fn build(self) -> StaticLayout {
        let LayoutBuilder { fields } = self;
        let fields: SmallVec<[_; 3]> = fields.into_iter().filter(|field| !field.is_empty()).collect();
        let fields: SlimSmallSliceBox<MemcpyField, 3> = fields.into();
        let bsatn_length = fields.last().map(|last| last.bsatn_offset + last.length).unwrap_or(0);

        StaticLayout { bsatn_length, fields }
    }

    fn current_field(&self) -> &MemcpyField {
        self.fields.last().unwrap()
    }

    fn current_field_mut(&mut self) -> &mut MemcpyField {
        self.fields.last_mut().unwrap()
    }

    fn next_bflatn_offset(&self) -> u16 {
        let last = self.current_field();
        last.bflatn_offset + last.length
    }

    fn next_bsatn_offset(&self) -> u16 {
        let last = self.current_field();
        last.bsatn_offset + last.length
    }

    fn visit_product(&mut self, product: ProductTypeLayoutView) -> Option<()> {
        let base_bflatn_offset = self.next_bflatn_offset();
        for elt in product.elements.iter() {
            self.visit_product_element(elt, base_bflatn_offset)?;
        }
        Some(())
    }

    fn visit_product_element(&mut self, elt: &ProductTypeElementLayout, product_base_offset: u16) -> Option<()> {
        let elt_offset = product_base_offset + elt.offset;
        let next_bflatn_offset = self.next_bflatn_offset();
        if next_bflatn_offset != elt_offset {
            // Padding between previous element and this element,
            // so start a new field.
            //
            // Note that this is the only place we have to reason about alignment and padding
            // because the enclosing `ProductTypeLayout` has already computed valid aligned offsets
            // for the elements.

            let bsatn_offset = self.next_bsatn_offset();
            self.fields.push(MemcpyField {
                bsatn_offset,
                bflatn_offset: elt_offset,
                length: 0,
            });
        }
        self.visit_value(&elt.ty)
    }

    fn visit_value(&mut self, val: &AlgebraicTypeLayout) -> Option<()> {
        match val {
            AlgebraicTypeLayout::Sum(sum) => self.visit_sum(sum),
            AlgebraicTypeLayout::Product(prod) => self.visit_product(prod.view()),
            AlgebraicTypeLayout::Primitive(prim) => {
                self.visit_primitive(prim);
                Some(())
            }

            // Var-len types (obviously) don't have a known BSATN length,
            // so fail.
            AlgebraicTypeLayout::VarLen(_) => None,
        }
    }

    fn visit_sum(&mut self, sum: &SumTypeLayout) -> Option<()> {
        // If the sum has no variants, it's the never type, so there's no point in computing a layout.
        let first_variant = sum.variants.first()?;

        let variant_layout = |variant: &SumTypeVariantLayout| {
            let mut builder = LayoutBuilder::new_builder();
            builder.visit_value(&variant.ty)?;
            Some(builder.build())
        };

        // Check that the variants all have the same `StaticLayout`.
        // If they don't, bail.
        let first_variant_layout = variant_layout(first_variant)?;
        for later_variant in &sum.variants[1..] {
            let later_variant_layout = variant_layout(later_variant)?;
            if later_variant_layout != first_variant_layout {
                return None;
            }
        }

        if first_variant_layout.bsatn_length == 0 {
            // For C-style enums (those without payloads),
            // simply serialize the tag and move on.
            self.current_field_mut().length += 1;
            return Some(());
        }

        // Now that we've reached this point, we know that `first_variant_layout`
        // applies to the values of all the variants.

        let tag_bflatn_offset = self.next_bflatn_offset();
        let payload_bflatn_offset = tag_bflatn_offset + sum.payload_offset;

        let tag_bsatn_offset = self.next_bsatn_offset();
        let payload_bsatn_offset = tag_bsatn_offset + 1;

        // Serialize the tag, consolidating into the previous memcpy if possible.
        self.visit_primitive(&PrimitiveType::U8);

        if sum.payload_offset > 1 {
            // Add an empty marker field to keep track of padding.
            self.fields.push(MemcpyField {
                bflatn_offset: payload_bflatn_offset,
                bsatn_offset: payload_bsatn_offset,
                length: 0,
            });
        } // Otherwise, nothing to do.

        // Lay out the variants.
        // Since all variants have the same layout, we just use the first one.
        self.visit_value(&first_variant.ty)?;

        Some(())
    }

    fn visit_primitive(&mut self, prim: &PrimitiveType) {
        self.current_field_mut().length += prim.size() as u16
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{blob_store::HashMapBlobStore, page_pool::PagePool};
    use proptest::prelude::*;
    use spacetimedb_sats::{bsatn, proptest::generate_typed_row, AlgebraicType, ProductType};

    fn assert_expected_layout(ty: ProductType, bsatn_length: u16, fields: &[(u16, u16, u16)]) {
        let expected_layout = StaticLayout {
            bsatn_length,
            fields: fields
                .iter()
                .copied()
                .map(|(bflatn_offset, bsatn_offset, length)| MemcpyField {
                    bflatn_offset,
                    bsatn_offset,
                    length,
                })
                .collect::<SmallVec<_>>()
                .into(),
        };
        let row_type = RowTypeLayout::from(ty.clone());
        let Some(computed_layout) = StaticLayout::for_row_type(&row_type) else {
            panic!("assert_expected_layout: Computed `None` for row {row_type:#?}\nExpected:{expected_layout:#?}");
        };
        assert_eq!(
            computed_layout, expected_layout,
            "assert_expected_layout: Computed layout (left) doesn't match expected (right) for {ty:?}",
        );
    }

    #[test]
    fn known_types_expected_layout_plain() {
        for prim in [
            AlgebraicType::Bool,
            AlgebraicType::U8,
            AlgebraicType::I8,
            AlgebraicType::U16,
            AlgebraicType::I16,
            AlgebraicType::U32,
            AlgebraicType::I32,
            AlgebraicType::U64,
            AlgebraicType::I64,
            AlgebraicType::U128,
            AlgebraicType::I128,
            AlgebraicType::U256,
            AlgebraicType::I256,
        ] {
            let size = AlgebraicTypeLayout::from(prim.clone()).size() as u16;
            assert_expected_layout(ProductType::from([prim]), size, &[(0, 0, size)]);
        }
    }

    #[test]
    fn known_types_expected_layout_complex() {
        for (ty, bsatn_length, fields) in [
            (ProductType::new([].into()), 0, &[][..]),
            (
                ProductType::from([AlgebraicType::sum([
                    AlgebraicType::U8,
                    AlgebraicType::I8,
                    AlgebraicType::Bool,
                ])]),
                2,
                // In BFLATN, sums have padding after the tag to the max alignment of any variant payload.
                // In this case, 0 bytes of padding, because all payloads are aligned to 1.
                // Since there's no padding, the memcpys can be consolidated.
                &[(0, 0, 2)][..],
            ),
            (
                ProductType::from([AlgebraicType::sum([
                    AlgebraicType::product([
                        AlgebraicType::U8,
                        AlgebraicType::U8,
                        AlgebraicType::U8,
                        AlgebraicType::U8,
                    ]),
                    AlgebraicType::product([AlgebraicType::U16, AlgebraicType::U16]),
                    AlgebraicType::U32,
                ])]),
                5,
                // In BFLATN, sums have padding after the tag to the max alignment of any variant payload.
                // In this case, 3 bytes of padding.
                &[(0, 0, 1), (4, 1, 4)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::sum([AlgebraicType::U128, AlgebraicType::I128]),
                    AlgebraicType::U32,
                ]),
                21,
                // In BFLATN, sums have padding after the tag to the max alignment of any variant payload.
                // In this case, 15 bytes of padding.
                &[(0, 0, 1), (16, 1, 20)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::sum([AlgebraicType::U256, AlgebraicType::I256]),
                    AlgebraicType::U32,
                ]),
                37,
                // In BFLATN, sums have padding after the tag to the max alignment of any variant payload.
                // In this case, 15 bytes of padding.
                &[(0, 0, 1), (32, 1, 36)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::U256,
                    AlgebraicType::U128,
                    AlgebraicType::U64,
                    AlgebraicType::U32,
                    AlgebraicType::U16,
                    AlgebraicType::U8,
                ]),
                63,
                &[(0, 0, 63)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::U8,
                    AlgebraicType::U16,
                    AlgebraicType::U32,
                    AlgebraicType::U64,
                    AlgebraicType::U128,
                ]),
                31,
                &[(0, 0, 1), (2, 1, 30)][..],
            ),
            // Make sure sums with no variant data are handled correctly.
            (
                ProductType::from([AlgebraicType::sum([AlgebraicType::product::<[AlgebraicType; 0]>([])])]),
                1,
                &[(0, 0, 1)][..],
            ),
            (
                ProductType::from([AlgebraicType::sum([
                    AlgebraicType::product::<[AlgebraicType; 0]>([]),
                    AlgebraicType::product::<[AlgebraicType; 0]>([]),
                ])]),
                1,
                &[(0, 0, 1)][..],
            ),
            // Various experiments with 1-byte-aligned payloads.
            // These are particularly nice for memcpy consolidation as there's no padding.
            (
                ProductType::from([AlgebraicType::sum([
                    AlgebraicType::product([AlgebraicType::U8, AlgebraicType::U8]),
                    AlgebraicType::product([AlgebraicType::Bool, AlgebraicType::Bool]),
                ])]),
                3,
                &[(0, 0, 3)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::sum([AlgebraicType::Bool, AlgebraicType::U8]),
                    AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::Bool]),
                ]),
                4,
                &[(0, 0, 4)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::U16,
                    AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::Bool]),
                    AlgebraicType::U16,
                ]),
                6,
                &[(0, 0, 6)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::U32,
                    AlgebraicType::sum([AlgebraicType::U16, AlgebraicType::I16]),
                    AlgebraicType::U32,
                ]),
                11,
                &[(0, 0, 5), (6, 5, 6)][..],
            ),
        ] {
            assert_expected_layout(ty, bsatn_length, fields);
        }
    }

    #[test]
    fn known_types_not_applicable() {
        for ty in [
            AlgebraicType::String,
            AlgebraicType::bytes(),
            AlgebraicType::never(),
            AlgebraicType::array(AlgebraicType::U16),
            AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::U16]),
        ] {
            let layout = RowTypeLayout::from(ProductType::from([ty]));
            if let Some(computed) = StaticLayout::for_row_type(&layout) {
                panic!("Expected row type not to have a constant BSATN layout!\nRow type: {layout:#?}\nBSATN layout: {computed:#?}");
            }
        }
    }

    proptest! {
        // The tests `known_bsatn_same_as_bflatn_from`
        // and `known_bflatn_same_as_pv_from` generate a lot of rejects,
        // as a vast majority of the space of `ProductType` does not have a fixed BSATN length.
        // Writing a proptest generator which produces only types that have a fixed BSATN length
        // seems hard, because we'd have to generate sums with known matching layouts,
        // so we just bump the `max_global_rejects` up as high as it'll go and move on with our lives.
        //
        // Note that I (pgoldman 2024-03-21) tried modifying `generate_typed_row`
        // to not emit `String`, `Array` or `Map` types (the trivially var-len types),
        // but did not see a meaningful decrease in the number of rejects.
        // This is because a majority of the var-len BSATN types in the `generate_typed_row` space
        // are due to sums with inconsistent payload layouts.
        //
        // We still include the test `known_bsatn_same_as_bsatn_from`
        // because it tests row types not covered in `known_types_expected_layout`,
        // especially larger types with unusual sequences of aligned fields.
        #![proptest_config(ProptestConfig { max_global_rejects: 65536, ..Default::default()})]

        #[test]
        fn known_bsatn_same_as_bflatn_from((ty, val) in generate_typed_row()) {
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let mut table = crate::table::test::table(ty);
            let Some(static_layout) = table.static_layout().cloned() else {
                // `ty` has a var-len member or a sum with different payload lengths,
                // so the fast path doesn't apply.
                return Err(TestCaseError::reject("Var-length type"));
            };

            let (_, row_ref) = table.insert(&pool, &mut blob_store, &val).unwrap();
            let bytes = row_ref.get_row_data();

            let slow_path = bsatn::to_vec(&row_ref).unwrap();

            let fast_path = unsafe {
                static_layout.serialize_row_into_vec(bytes)
            };

            let mut fast_path2 = Vec::new();
            unsafe {
                static_layout.serialize_row_extend(&mut fast_path2, bytes)
            };

            assert_eq!(slow_path, fast_path);
            assert_eq!(slow_path, fast_path2);
        }

        #[test]
        fn known_bflatn_same_as_pv_from((ty, val) in generate_typed_row()) {
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let mut table = crate::table::test::table(ty);
            let Some(static_layout) = table.static_layout().cloned() else {
                // `ty` has a var-len member or a sum with different payload lengths,
                // so the fast path doesn't apply.
                return Err(TestCaseError::reject("Var-length type"));
            };
            let bsatn = bsatn::to_vec(&val).unwrap();

            let (_, row_ref) = table.insert(&pool, &mut blob_store, &val).unwrap();
            let slow_path = row_ref.get_row_data();

            let mut fast_path = vec![0u8; slow_path.len()];
            unsafe {
                static_layout.deserialize_row_into(&mut fast_path, &bsatn);
            };

            assert_eq!(slow_path, fast_path);
        }
    }
}
