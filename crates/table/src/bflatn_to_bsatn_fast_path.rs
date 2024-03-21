//! This module implements a fast path for serializing certain types from BFLATN to BSATN.
//!
//! The key insight is that a majority of row types in a game will have a known fixed length,
//! with no variable-length members.
//! BFLATN is designed with this in mind, storing fixed-length portions of rows inline,
//! at the expense of an indirection to reach var-length columns like strings.
//! A majority of these types will also have a fixed BSATN length,
//! but note that BSATN stores sum values (enums) without padding,
//! so row types which contain sums may not have a fixed BSATN length
//! if the sum's variants have different "live" unpadded lengths.
//!
//! For row types with fixed BSATN lengths, we can reduce the BFLATN -> BSATN conversion
//! to a series of `memcpy`s, skipping over padding sequences.

use crate::{
    indexes::Bytes,
    layout::{
        AlgebraicTypeLayout, HasLayout, PrimitiveType, ProductTypeElementLayout, ProductTypeLayout, RowTypeLayout,
        SumTypeLayout, SumTypeVariantLayout,
    },
    util::{range_move, slice_assume_init_ref},
};

/// A precomputed BSATN layout for a type whose encoded length is a known constant,
/// enabling fast BFLATN -> BSATN conversion.
#[derive(PartialEq, Eq, Debug)]
pub struct KnownBsatnLayout {
    /// The length of the encoded BSATN representation of a row of this type,
    /// in bytes.
    bsatn_length: usize,

    fields: Vec<MemcpyField>,
}

impl KnownBsatnLayout {
    /// # Safety
    ///
    /// - `buf` must be at least `self.bsatn_length` long.
    /// - `row` must store a valid, initialized instance of the BFLATN row type
    ///   for which `self` was computed.
    pub unsafe fn serialize_row_into(&self, buf: &mut [u8], row: &Bytes) {
        debug_assert!(buf.len() >= self.bsatn_length);
        for field in &self.fields {
            // Safety: forward caller requirements.
            unsafe { field.copy(buf, row) };
        }
    }

    /// Construct a `KnownBsatnLayout` for converting BFLATN rows of `row_type` into BSATN.
    ///
    /// Returns `None` if `row_type` contains a column which does not have a constant length in BSATN,
    /// either a [`VarLenType`]
    /// or a [`SumTypeLayout`] whose variants do not have the same "live" unpadded length.
    pub fn for_row_type(row_type: &RowTypeLayout) -> Option<Self> {
        let mut builder = LayoutBuilder::default();
        builder.visit_product(row_type.product())?;
        Some(builder.build())
    }
}

/// An identifier for a series of bytes within a BFLATN row
/// which can be directly copied into an output BSATN buffer
/// with a known length and offset.
///
/// Within the row type's BFLATN layout, `row[bflatn_offset .. (bflatn_offset + length)]`
/// must not contain any padding bytes,
/// i.e. all of those bytes must be fully initialized if the row is initialized.
#[derive(PartialEq, Eq, Debug)]
struct MemcpyField {
    /// Offset in the BFLATN row from which to begin `memcpy`ing, in bytes.
    bflatn_offset: usize,

    /// Offset in the BSATN buffer to which to begin `memcpy`ing, in bytes.
    bsatn_offset: usize,

    /// Length to `memcpy`, in bytes.
    length: usize,
}

impl MemcpyField {
    /// # Safety
    ///
    /// - `buf` must be at least `self.bsatn_offset + self.length` long.
    /// - `row` must be at least `self.bflatn_offset + self.length` long.
    /// - `row[self.bflatn_offset .. self.bflatn_offset + length]` must all be initialized.
    unsafe fn copy(&self, buf: &mut [u8], row: &Bytes) {
        // Safety: forward caller requirement #1.
        let to = unsafe { buf.get_unchecked_mut(range_move(0..self.length, self.bsatn_offset)) };
        // Safety: forward caller requirement #2.
        let from = unsafe { row.get_unchecked(range_move(0..self.length, self.bflatn_offset)) };
        // Safety: forward caller requirement #3.
        let from = unsafe { slice_assume_init_ref(from) };
        to.copy_from_slice(from);
    }

    fn is_empty(&self) -> bool {
        self.length == 0
    }
}

struct LayoutBuilder {
    /// Always at least one element.
    fields: Vec<MemcpyField>,
}

impl Default for LayoutBuilder {
    fn default() -> Self {
        Self {
            fields: vec![MemcpyField {
                bflatn_offset: 0,
                bsatn_offset: 0,
                length: 0,
            }],
        }
    }
}

impl LayoutBuilder {
    fn build(self) -> KnownBsatnLayout {
        let LayoutBuilder { fields } = self;
        let fields: Vec<_> = fields.into_iter().filter(|field| !field.is_empty()).collect();
        let bsatn_length = fields.last().map(|last| last.bsatn_offset + last.length).unwrap_or(0);
        KnownBsatnLayout { bsatn_length, fields }
    }

    fn current_field(&self) -> &MemcpyField {
        self.fields.last().unwrap()
    }

    fn current_field_mut(&mut self) -> &mut MemcpyField {
        self.fields.last_mut().unwrap()
    }

    fn next_bflatn_offset(&self) -> usize {
        let last = self.current_field();
        last.bflatn_offset + last.length
    }

    fn next_bsatn_offset(&self) -> usize {
        let last = self.current_field();
        last.bsatn_offset + last.length
    }

    fn visit_product(&mut self, product: &ProductTypeLayout) -> Option<()> {
        let base_bflatn_offset = self.next_bflatn_offset();
        for elt in product.elements.iter() {
            self.visit_product_element(elt, base_bflatn_offset)?;
        }
        Some(())
    }

    fn visit_product_element(&mut self, elt: &ProductTypeElementLayout, product_base_offset: usize) -> Option<()> {
        let elt_offset = product_base_offset + elt.offset as usize;
        let next_bflatn_offset = self.next_bflatn_offset();
        if next_bflatn_offset != elt_offset {
            // Padding between previous element and this element,
            // so start a new field.

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
            AlgebraicTypeLayout::Product(prod) => self.visit_product(prod),
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
            let mut builder = LayoutBuilder::default();
            builder.visit_value(&variant.ty)?;
            Some(builder.build())
        };

        // Check that the variants all have the same `KnownBsatnLayout`.
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
        // Do a bit of hackery to re-order the tag, since BFLATN stores `(payload, tag)`,
        // but BSATN stores `(tag, payload)`,
        // then splice the `first_variant_layout` into `self`.

        let payload_bflatn_offset = self.next_bflatn_offset();
        let tag_bflatn_offset = payload_bflatn_offset + sum.offset_of_tag();

        let tag_bsatn_offset = self.next_bsatn_offset();
        let payload_bsatn_offset = tag_bsatn_offset + 1;

        self.fields.push(MemcpyField {
            bflatn_offset: tag_bflatn_offset,
            bsatn_offset: tag_bsatn_offset,
            length: 1,
        });

        for payload_field in first_variant_layout.fields {
            self.fields.push(MemcpyField {
                bflatn_offset: payload_bflatn_offset + payload_field.bflatn_offset,
                bsatn_offset: payload_bsatn_offset + payload_field.bsatn_offset,
                length: payload_field.length,
            });
        }

        // Finally, start a new field which skips over the tag.
        // This field will almost certainly end up empty,
        // as there will generally be padding following the tag in `sum`,
        // but that's okay, because `Self::build` strips empty fields.
        let next_bsatn_offset = self.next_bsatn_offset();
        self.fields.push(MemcpyField {
            bflatn_offset: tag_bflatn_offset + 1,
            bsatn_offset: next_bsatn_offset,
            length: 0,
        });

        Some(())
    }

    fn visit_primitive(&mut self, prim: &PrimitiveType) {
        self.current_field_mut().length += prim.size()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{blob_store::HashMapBlobStore, proptest_sats::generate_typed_row};
    use proptest::prelude::*;
    use spacetimedb_sats::{bsatn, AlgebraicType, ProductType};

    fn assert_expected_layout(ty: ProductType, bsatn_length: usize, fields: &[(usize, usize, usize)]) {
        let expected_layout = KnownBsatnLayout {
            bsatn_length,
            fields: fields
                .iter()
                .copied()
                .map(|(bflatn_offset, bsatn_offset, length)| MemcpyField {
                    bflatn_offset,
                    bsatn_offset,
                    length,
                })
                .collect(),
        };
        let row_type = RowTypeLayout::from(ty);
        let Some(computed_layout) = KnownBsatnLayout::for_row_type(&row_type) else {
            panic!("assert_expected_layout: Computed `None` for row {row_type:#?}\nExpected:{expected_layout:#?}");
        };
        assert_eq!(
            computed_layout, expected_layout,
            "assert_expected_layout: Computed layout (left) does not match expected layout (right)"
        );
    }

    #[test]
    fn known_types_expected_layout() {
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
        ] {
            let size = AlgebraicTypeLayout::from(prim.clone()).size();
            assert_expected_layout(ProductType::from([prim]), size, &[(0, 0, size)]);
        }

        for (ty, bsatn_length, fields) in [
            (ProductType::new(vec![]), 0, &[][..]),
            (
                ProductType::from([AlgebraicType::sum([
                    AlgebraicType::U8,
                    AlgebraicType::I8,
                    AlgebraicType::Bool,
                ])]),
                2,
                // Sums get wonky layouts
                // because BFLATN and BSATN store the tag and the payload in opposite orders.
                &[(1, 0, 1), (0, 1, 1)][..],
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
                // Sums get wonky layouts
                // because BFLATN and BSATN store the tag and the payload in opposite orders.
                &[(4, 0, 1), (0, 1, 4)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::sum([AlgebraicType::U128, AlgebraicType::I128]),
                    AlgebraicType::U32,
                ]),
                21,
                // Sums get wonky layouts
                // because BFLATN and BSATN store the tag and the payload in opposite orders.
                &[(16, 0, 1), (0, 1, 16), (32, 17, 4)][..],
            ),
            (
                ProductType::from([
                    AlgebraicType::U128,
                    AlgebraicType::U64,
                    AlgebraicType::U32,
                    AlgebraicType::U16,
                    AlgebraicType::U8,
                ]),
                31,
                &[(0, 0, 31)][..],
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
            AlgebraicType::map(AlgebraicType::U8, AlgebraicType::I8),
            AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::U16]),
        ] {
            let layout = RowTypeLayout::from(ProductType::from([ty]));
            if let Some(computed) = KnownBsatnLayout::for_row_type(&layout) {
                panic!("Expected row type not to have a constant BSATN layout!\nRow type: {layout:#?}\nBSATN layout: {computed:#?}");
            }
        }
    }

    proptest! {
        // The test `known_bsatn_same_as_bflatn_from` generates a lot of rejects,
        // as a vast majority of the space of `ProductType` does not have a fixed BSATN length.
        // Writing a proptest generator which produces only types that have a fixed BSATN length
        // seems hard, because we'd have to generate sums with known matching layouts,
        // so we just bump the `max_global_rejects` up as high as it'll go and move on with our lives.
        #![proptest_config(ProptestConfig { max_global_rejects: 65536, ..Default::default()})]

        #[test]
        fn known_bsatn_same_as_bflatn_from((ty, val) in generate_typed_row()) {
            let mut blob_store = HashMapBlobStore::default();
            let mut table = crate::table::test::table(ty);
            let Some(bsatn_layout) = KnownBsatnLayout::for_row_type(table.row_layout()) else {
                // `ty` has a var-len member or a sum with different payload lengths,
                // so the fast path doesn't apply.
                return Err(TestCaseError::reject("Var-length type"));
            };

            let (_, ptr) = table.insert(&mut blob_store, &val).unwrap();

            let row_ref = table.get_row_ref(&blob_store, ptr).unwrap();
            let slow_path = bsatn::to_vec(&row_ref).unwrap();

            let (page, offset) = row_ref.page_and_offset();
            let bytes = page.get_row_data(offset, table.row_layout().size());

            let mut fast_path = vec![0u8; bsatn_layout.bsatn_length];
            unsafe {
                bsatn_layout.serialize_row_into(&mut fast_path, bytes);
            }

            assert_eq!(slow_path, fast_path);
        }
    }
}
