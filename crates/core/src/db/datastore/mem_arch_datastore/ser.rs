//! Provides the functions [`write_row_to_pages(pages, blob_store, ty, val)`]
//! and [`write_row_to_page(page, blob_store, visitor, ty, val)`]
//! which write `val: ProductValue` typed at `ty` to `page` and `pages` respectively.

use super::{
    blob_store::BlobStore,
    indexes::{Bytes, PageOffset, RowPointer, SquashedOffset},
    layout::{
        align_to, required_var_len_granules_for_row, AlgebraicTypeLayout, HasLayout, ProductTypeLayout, RowTypeLayout,
        SumTypeLayout, VarLenType,
    },
    page::{Page, VarView},
    pages::Pages,
    util::{maybe_uninit_write_slice, range_add},
    var_len::{visit_var_len_assume_init, VarLenMembers, VarLenRef},
};
use spacetimedb_sats::{AlgebraicValue, ProductValue, SumValue};

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
#[allow(clippy::result_unit_err)] // TODO(error-handling,integration): useful error type
pub unsafe fn write_row_to_pages(
    pages: &mut Pages,
    visitor: &impl VarLenMembers,
    blob_store: &mut dyn BlobStore,
    ty: &RowTypeLayout,
    val: &ProductValue,
    squashed_offset: SquashedOffset,
) -> Result<RowPointer, ()> {
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
#[allow(clippy::result_unit_err)] // TODO(error-handling,integration): useful error type
pub unsafe fn write_row_to_page(
    page: &mut Page,
    blob_store: &mut dyn BlobStore,
    visitor: &impl VarLenMembers,
    ty: &RowTypeLayout,
    val: &ProductValue,
) -> Result<PageOffset, ()> {
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
    /// SAFETY: The `visitor` must be proper for the row type.
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
                self.var_view
                    // TODO(error-handling,blocker): handle errors when the blob-store fails to alloc.
                    // Prereq: talk to Tyler about fallibility in the blob-store.
                    .write_large_blob_hash_to_granule(blob_store, &value, vlr);
            }
        }
    }

    /// Write an `val`, an [`AlgebraicValue`], typed at `ty`, to the buffer.
    fn write_value(&mut self, ty: &AlgebraicTypeLayout, val: &AlgebraicValue) -> Result<(), ()> {
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
            (&AlgebraicTypeLayout::String, AlgebraicValue::String(val)) => {
                self.write_var_len_obj(val.clone().into_bytes())?
            }

            // For array and maps, we reserve space for a `VarLenRef`
            // and push the bytes, after BSATN encoding, as a var-len object.
            (AlgebraicTypeLayout::VarLen(VarLenType::Array(_)), val @ AlgebraicValue::Array(_))
            | (AlgebraicTypeLayout::VarLen(VarLenType::Map(_)), val @ AlgebraicValue::Map(_)) => {
                // TODO(perf): `with_capacity`?
                let mut bytes = Vec::new();
                val.encode(&mut bytes);
                self.write_var_len_obj(bytes)?;
            }

            // TODO(error-handling): return type error
            (ty, val) => panic!(
                "AlgebraicValue is not valid instance of AlgebraicTypeLayout: {:?} should be of type {:?}",
                val, ty,
            ),
        }

        self.curr_offset = align_to(self.curr_offset, ty_alignment);

        Ok(())
    }

    /// Write a `val`, a [`SumValue`], typed at `ty`, to the buffer.
    fn write_sum(&mut self, ty: &SumTypeLayout, val: &SumValue) -> Result<(), ()> {
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
    fn write_product(&mut self, ty: &ProductTypeLayout, val: &ProductValue) -> Result<(), ()> {
        for (elt_ty, elt) in ty.elements.iter().zip(val.elements.iter()) {
            self.write_value(&elt_ty.ty, elt)?;
        }
        Ok(())
    }

    /// Write a var-len object where the object contents are `obj_bytes`.
    fn write_var_len_obj(&mut self, obj_bytes: Vec<u8>) -> Result<(), ()> {
        // Write `obj_bytes` to the page. The handle is `vlr`.
        let (vlr, in_blob) = self.var_view.alloc_for_slice(&obj_bytes)?;
        // Write `vlr` to the fixed part.
        self.write_var_len_ref(vlr);

        // For large blobs, we'll need to come back and write the blob hash
        // and insert to blob store.
        if in_blob {
            self.large_blob_insertions.push((vlr, obj_bytes));
        }

        // Keep track of how many var len objects we've added so far
        // so that we can free them on failure.
        self.last_allocated_var_len_index += 1;
        Ok(())
    }

    /// Write `bytes: &[u8; N]` starting at the current offset
    /// and advance the offset by `N`.
    fn write_bytes<const N: usize>(&mut self, bytes: &[u8; N]) {
        maybe_uninit_write_slice(&mut self.fixed_buf[range_add(0..N, self.curr_offset)], bytes);
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

    /// Write a `VarLenRef` to the fixed buffer and advance the `curr_offset`.
    fn write_var_len_ref(&mut self, val: VarLenRef) {
        self.write_u16(val.length_in_bytes);
        self.write_u16(val.first_granule.0);
    }
}

#[cfg(test)]
pub mod test {
    use super::super::{blob_store::HashMapBlobStore, de::read_row_from_page, row_vars_simple::row_type_visitor};
    use super::*;
    use proptest::{
        collection::{vec, SizeRange},
        prelude::*,
        prop_assert_eq, prop_oneof, proptest,
        strategy::Just,
        strategy::{BoxedStrategy, Strategy},
    };
    use spacetimedb_sats::{
        AlgebraicType, ArrayValue, BuiltinType, MapType, MapValue, ProductType, ProductValue, SumType, F32, F64,
    };

    /// Generates leaf `AlgebraicType`s.
    fn generate_primitive_algebraic_type() -> impl Strategy<Value = AlgebraicType> {
        prop_oneof![
            Just(AlgebraicType::Bool),
            Just(AlgebraicType::U8),
            Just(AlgebraicType::I8),
            Just(AlgebraicType::U16),
            Just(AlgebraicType::I16),
            Just(AlgebraicType::U32),
            Just(AlgebraicType::I32),
            Just(AlgebraicType::U64),
            Just(AlgebraicType::I64),
            Just(AlgebraicType::U128),
            Just(AlgebraicType::I128),
            Just(AlgebraicType::F32),
            Just(AlgebraicType::F64),
            Just(AlgebraicType::String),
            Just(AlgebraicType::unit()),
        ]
    }

    /// Generates `AlgebraicType`s including recursive ones.
    pub fn generate_algebraic_type() -> impl Strategy<Value = AlgebraicType> {
        generate_primitive_algebraic_type().prop_recursive(4, 16, 16, |gen_element| {
            prop_oneof![
                gen_element.clone().prop_map(AlgebraicType::array),
                (gen_element.clone(), gen_element.clone()).prop_map(|(key, val)| AlgebraicType::map(key, val)),
                // No need for field or variant names.

                // No need to generate units here;
                // we already generate them in `generate_primitive_algebraic_type`.
                vec(gen_element.clone().prop_map_into(), 1..=16).prop_map(AlgebraicType::product),
                // Do not generate nevers here; we can't store never in a page.
                vec(gen_element.clone().prop_map_into(), 1..=16).prop_map(AlgebraicType::sum),
            ]
        })
    }

    /// Generates a `ProductType` that is good as a row type.
    pub fn generate_row_type(range: impl Into<SizeRange>) -> impl Strategy<Value = ProductType> {
        vec(generate_algebraic_type().prop_map_into(), range).prop_map_into()
    }

    /// Generates an `AlgebraicValue` for values `Val: Arbitrary`.
    fn generate_primitive<Val: Arbitrary + Into<AlgebraicValue> + 'static>() -> BoxedStrategy<AlgebraicValue> {
        any::<Val>().prop_map(Into::into).boxed()
    }

    /// Generates an `AlgebraicValue` typed at `ty`.
    pub fn generate_algebraic_value(ty: AlgebraicType) -> impl Strategy<Value = AlgebraicValue> {
        match ty {
            AlgebraicType::Bool => generate_primitive::<bool>(),
            AlgebraicType::I8 => generate_primitive::<i8>(),
            AlgebraicType::U8 => generate_primitive::<u8>(),
            AlgebraicType::I16 => generate_primitive::<i16>(),
            AlgebraicType::U16 => generate_primitive::<u16>(),
            AlgebraicType::I32 => generate_primitive::<i32>(),
            AlgebraicType::U32 => generate_primitive::<u32>(),
            AlgebraicType::I64 => generate_primitive::<i64>(),
            AlgebraicType::U64 => generate_primitive::<u64>(),
            AlgebraicType::I128 => generate_primitive::<i128>(),
            AlgebraicType::U128 => generate_primitive::<u128>(),
            AlgebraicType::F32 => generate_primitive::<f32>(),
            AlgebraicType::F64 => generate_primitive::<f64>(),
            AlgebraicType::String => generate_primitive::<String>(),

            AlgebraicType::Builtin(BuiltinType::Array(ty)) => generate_array_value(*ty.elem_ty).prop_map_into().boxed(),

            AlgebraicType::Builtin(BuiltinType::Map(ty)) => generate_map_value(*ty).prop_map_into().boxed(),

            AlgebraicType::Product(ty) => generate_product_value(ty).prop_map_into().boxed(),

            AlgebraicType::Sum(ty) => generate_sum_value(ty).prop_map_into().boxed(),

            AlgebraicType::Ref(_) => unreachable!(),
        }
    }

    /// Generates a `ProductValue` typed at `ty`.
    pub fn generate_product_value(ty: ProductType) -> impl Strategy<Value = ProductValue> {
        ty.elements
            .into_iter()
            .map(|elem| generate_algebraic_value(elem.algebraic_type))
            .collect::<Vec<_>>()
            .prop_map(|elements| ProductValue { elements })
    }

    /// Generates a `SumValue` typed at `ty`.
    fn generate_sum_value(ty: SumType) -> impl Strategy<Value = SumValue> {
        // A dependent problem, generate a tag
        // and then generate a value typed at the tag' data type.
        (0..ty.variants.len()).prop_flat_map(move |tag: usize| {
            let variant_ty = ty.variants[tag].clone();
            let gen_variant = generate_algebraic_value(variant_ty.algebraic_type);
            gen_variant.prop_map(move |value| SumValue {
                tag: tag as u8,
                value: Box::new(value),
            })
        })
    }

    /// Generates a `MapValue` typed at `ty`.
    fn generate_map_value(ty: MapType) -> impl Strategy<Value = MapValue> {
        vec(
            (generate_algebraic_value(ty.key_ty), generate_algebraic_value(ty.ty)),
            0..=16,
        )
        .prop_map(|entries| entries.into_iter().collect())
    }

    /// Generates an array value given an element generator `gen_elem`.
    fn generate_array_of<S>(gen_elem: S) -> BoxedStrategy<ArrayValue>
    where
        S: Strategy + 'static,
        Vec<S::Value>: 'static + Into<ArrayValue>,
    {
        vec(gen_elem, 0..=16).prop_map_into().boxed()
    }

    /// Generates an array value with elements typed at `ty`.
    fn generate_array_value(ty: AlgebraicType) -> BoxedStrategy<ArrayValue> {
        match ty {
            AlgebraicType::Bool => generate_array_of(any::<bool>()),
            AlgebraicType::I8 => generate_array_of(any::<i8>()),
            AlgebraicType::U8 => generate_array_of(any::<u8>()),
            AlgebraicType::I16 => generate_array_of(any::<i16>()),
            AlgebraicType::U16 => generate_array_of(any::<u16>()),
            AlgebraicType::I32 => generate_array_of(any::<i32>()),
            AlgebraicType::U32 => generate_array_of(any::<u32>()),
            AlgebraicType::I64 => generate_array_of(any::<i64>()),
            AlgebraicType::U64 => generate_array_of(any::<u64>()),
            AlgebraicType::I128 => generate_array_of(any::<i128>()),
            AlgebraicType::U128 => generate_array_of(any::<u128>()),
            AlgebraicType::F32 => generate_array_of(any::<f32>().prop_map_into::<F32>()),
            AlgebraicType::F64 => generate_array_of(any::<f64>().prop_map_into::<F64>()),
            AlgebraicType::String => generate_array_of(any::<String>()),
            AlgebraicType::Product(ty) => generate_array_of(generate_product_value(ty)),
            AlgebraicType::Sum(ty) => generate_array_of(generate_sum_value(ty)),
            AlgebraicType::Builtin(BuiltinType::Array(ty)) => generate_array_of(generate_array_value(*ty.elem_ty)),
            AlgebraicType::Builtin(BuiltinType::Map(ty)) => generate_array_of(generate_map_value(*ty)),
            AlgebraicType::Ref(_) => unreachable!(),
        }
    }

    /// Generates a row type `ty` and a row value typed at `ty`.
    pub fn generate_typed_row() -> impl Strategy<Value = (ProductType, ProductValue)> {
        generate_row_type(0..=16).prop_flat_map(|ty| (Just(ty.clone()), generate_product_value(ty)))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2048))]
        #[test]
        fn av_serde_round_trip_through_page((ty, val) in generate_typed_row()) {
            let ty: RowTypeLayout = ty.into();
            let mut page = Page::new(ty.size());
            let visitor = row_type_visitor(&ty);
            let blob_store = &mut HashMapBlobStore::default();

            let offset = unsafe { write_row_to_page(&mut page, blob_store, &visitor, &ty, &val).unwrap() };

            let read_val = unsafe { read_row_from_page(&page, blob_store, offset, &ty) };

            prop_assert_eq!(val, read_val);
        }
    }
}
