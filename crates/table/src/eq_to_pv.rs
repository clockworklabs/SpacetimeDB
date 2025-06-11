//! Provides the function [`eq_row_in_page_to_pv(page, offset, pv, ty)`]
//! which, for `value = page/b.get_row_data(offset, fixed_row_size)` typed at `ty`,
//! and `pv`, a product value typed at `ty`,
//! compares `value` and `pv` for equality.

use crate::{
    bflatn_from::{read_tag, vlr_blob_bytes},
    blob_store::BlobStore,
    eq::BytesPage,
    indexes::PageOffset,
    layout::{align_to, AlgebraicTypeLayout, HasLayout as _, ProductTypeLayoutView, RowTypeLayout},
    page::Page,
    row_hash::{read_from_bytes, run_vlo_bytes},
    var_len::{VarLenGranule, VarLenRef},
};
use core::str;
use spacetimedb_sats::bsatn::{eq::eq_bsatn, Deserializer};
use spacetimedb_sats::{AlgebraicValue, ProductValue};

/// Equates row `lhs` in `page` with its fixed part starting at `fixed_offset` to `rhs`.
/// It is required for safety that `lhs` be typed at `ty`.
/// That is, row `lhs` has length `ty.size()` bytes,
/// is assumed to be typed at `ty` and must be valid for `ty`.
/// `rhs` should also be typed at `ty`, but this is only a logical requirement,
/// not a safety requirement.
///
/// Returns whether row `lhs` is equal to `rhs`.
///
/// # Safety
///
/// 1. `fixed_offset` is a valid offset for row `lhs` typed at `ty` in `page`.
/// 2. for any `vlr: VarLenRef` in the fixed parts of row `lhs`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
pub unsafe fn eq_row_in_page_to_pv(
    blob_store: &dyn BlobStore,
    page: &Page,
    fixed_offset: PageOffset,
    rhs: &ProductValue,
    ty: &RowTypeLayout,
) -> bool {
    // Context for the whole comparison.
    let mut ctx = EqCtx {
        lhs: BytesPage::new(page, fixed_offset, ty),
        blob_store,
        curr_offset: 0,
    };
    // Test for equality!
    // SAFETY:
    // 1. Per requirement 1., row `lhs` is valid at type `ty` and properly aligned for `ty`.
    //    Their fixed parts are defined as:
    //    `lhs = ctx.a/b.bytes[range_move(0..ty.size(), fixed_offset)]`
    //    as needed.
    // 2. for any `vlr: VarLenRef` stored in `lhs`,
    //   `vlr.first_offset` must either be `NULL` or point to a valid granule in `page`.
    unsafe { eq_product(&mut ctx, ty.product(), rhs) }
}

/// Comparison context used in the functions below.
#[derive(Clone, Copy)]
struct EqCtx<'page> {
    /// The view into the fixed part of row `lhs` in its page.
    lhs: BytesPage<'page>,
    /// The blob store that `lhs.page` uses for its large blob VLOs.
    blob_store: &'page dyn BlobStore,
    /// The current offset at which some sub-object of `lhs` exists.
    curr_offset: usize,
}

/// For every product field in `lhs = &ctx.lhs.bytes[range_move(0..ty.size(), *ctx.curr_offset)]`,
/// which is typed at `ty`,
/// equates `lhs`, including any var-len object, to the corresponding field in `rhs`
/// and advances the `ctx.curr_offset`.
///
/// SAFETY:
/// 1. `lhs` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `lhs`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `ctx.lhs.page`.
unsafe fn eq_product(ctx: &mut EqCtx<'_>, ty: ProductTypeLayoutView<'_>, rhs: &ProductValue) -> bool {
    let base_offset = ctx.curr_offset;
    ty.elements.len() == rhs.elements.len()
        && ty.elements.iter().zip(&*rhs.elements).all(|(elem_ty, rhs)| {
            ctx.curr_offset = base_offset + elem_ty.offset as usize;

            // SAFETY: By 1., `lhs` is valid at `ty`,
            // so it follows that valid and properly aligned sub-`lhs`s
            // are valid `elem_ty.ty`s.
            // By 2., and the above, it follows that sub-`lhs`s won't have dangling `VarLenRef`s.
            unsafe { eq_value(ctx, &elem_ty.ty, rhs) }
        })
}

/// For `lhs = &ctx.lhs.bytes[range_move(0..ty.size(), *ctx.curr_offset)]` typed at `ty`,
/// equates `lhs == rhs`, including any var-len objects,
/// and advances the `ctx.curr_offset`.
///
/// SAFETY:
/// 1. `lhs` must both be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr: VarLenRef` stored in `lhs`,
///    `vlr.first_offset` must either be `NULL` or point to a valid granule in `ctx.lhs.page`.
unsafe fn eq_value(ctx: &mut EqCtx<'_>, ty: &AlgebraicTypeLayout, rhs: &AlgebraicValue) -> bool {
    debug_assert_eq!(
        ctx.curr_offset,
        align_to(ctx.curr_offset, ty.align()),
        "curr_offset {} insufficiently aligned for type {:?}",
        ctx.curr_offset,
        ty
    );

    match (ty, rhs) {
        (AlgebraicTypeLayout::Sum(ty), AlgebraicValue::Sum(rhs)) => {
            // Read the tag of the sum value of `lhs`.
            let (tag_lhs, data_ty) = read_tag(ctx.lhs.bytes, ty, ctx.curr_offset);

            // The tags must match!
            if tag_lhs != rhs.tag {
                return false;
            }

            // Equate the variant data values.
            let curr_offset = ctx.curr_offset + ty.offset_of_variant_data(tag_lhs);
            ctx.curr_offset += ty.size();
            let mut ctx = EqCtx { curr_offset, ..*ctx };
            // SAFETY: `lhs` are valid at `ty` so given `tag_lhs`,
            // we know `data_lhs = &ctx.lhs.bytes[range_move(0..data_ty.size(), curr_offset))`
            // are valid at `data_ty`.
            // By 2., and the above, we also know that `data_lhs` won't have dangling `VarLenRef`s.
            unsafe { eq_value(&mut ctx, data_ty, &rhs.value) }
        }
        (AlgebraicTypeLayout::Product(ty), AlgebraicValue::Product(rhs)) => {
            // SAFETY: `lhs` is valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { eq_product(ctx, ty.view(), rhs) }
        }

        // The primitive types:
        // SAFETY(for all of the below): `lhs` is valid at `ty = T`.
        (&AlgebraicTypeLayout::Bool, AlgebraicValue::Bool(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::I8, AlgebraicValue::I8(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::U8, AlgebraicValue::U8(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::I16, AlgebraicValue::I16(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::U16, AlgebraicValue::U16(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::I32, AlgebraicValue::I32(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::U32, AlgebraicValue::U32(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::I64, AlgebraicValue::I64(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::U64, AlgebraicValue::U64(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::I128, AlgebraicValue::I128(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::U128, AlgebraicValue::U128(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::I256, AlgebraicValue::I256(rhs)) => unsafe { eq_at(ctx, &**rhs) },
        (&AlgebraicTypeLayout::U256, AlgebraicValue::U256(rhs)) => unsafe { eq_at(ctx, &**rhs) },
        (&AlgebraicTypeLayout::F32, AlgebraicValue::F32(rhs)) => unsafe { eq_at(ctx, rhs) },
        (&AlgebraicTypeLayout::F64, AlgebraicValue::F64(rhs)) => unsafe { eq_at(ctx, rhs) },

        // The var-len cases.
        (&AlgebraicTypeLayout::String, AlgebraicValue::String(rhs)) => {
            // SAFETY: `lhs` was valid at and aligned for `ty` (= `String`, as required).
            // These `ty` store a `vlr: VarLenRef` as their value,
            // so the range is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr.first_granule` was promised by the caller
            // to either be `NULL` or point to a valid granule in `ctx.lhs.page`.
            unsafe { eq_str(ctx, rhs) }
        }
        (AlgebraicTypeLayout::VarLen(_), AlgebraicValue::Array(_)) => {
            // SAFETY: `lhs` was valid at and aligned for `ty`.
            // This kind of `ty` stores a `vlr: VarLenRef` as its value,
            // so the range is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr.first_granule` were promised by the caller
            // to either be `NULL` or point to a valid granule in `ctx.lhs.page`.
            unsafe {
                run_vlo_bytes(
                    ctx.lhs.page,
                    ctx.lhs.bytes,
                    ctx.blob_store,
                    &mut ctx.curr_offset,
                    |mut bsatn| {
                        let lhs = Deserializer::new(&mut bsatn);
                        eq_bsatn(rhs, lhs)
                    },
                )
            }
        }
        _ => false,
    }
}

/// Equates `ctx.lhs`, known to store a string at `ctx.current_offset`, to `rhs`,
/// and advances `ctx.current_offset`.
///
/// SAFETY: `lhs = ctx.lhs.bytes[range_move(0..size_of::<VarLenRef>(), *curr_offset)]`
/// must be a valid `vlr = VarLenRef` and `&data` must be properly aligned for a `VarLenRef`.
/// The `vlr.first_granule` must be `NULL` or must point to a valid granule in `page`.
/// Moreover, `lhs` must be typed at `AlgebraicTypeLayout::String`.
unsafe fn eq_str(ctx: &mut EqCtx<'_>, rhs: &str) -> bool {
    // SAFETY: `value` was valid at and aligned for `ty = String`.
    // These `ty` store a `vlr: VarLenRef` as their fixed value.
    // The range thus is valid and properly aligned for `VarLenRef`.
    let vlr = unsafe { read_from_bytes::<VarLenRef>(ctx.lhs.bytes, &mut ctx.curr_offset) };

    if vlr.is_large_blob() {
        // SAFETY: As `vlr` is a blob, `vlr.first_granule` always points to a valid granule.
        let bytes = unsafe { vlr_blob_bytes(ctx.lhs.page, ctx.blob_store, vlr) };
        // SAFETY: For `ty = String`, the blob will always be valid UTF-8.
        rhs == unsafe { str::from_utf8_unchecked(bytes) }
    } else {
        // SAFETY: `vlr.first_granule` is either NULL or points to a valid granule.
        let lhs_chunks = unsafe { ctx.lhs.page.iter_vlo_data(vlr.first_granule) };
        let total_len = vlr.length_in_bytes as usize;

        // Don't bother checking the data if the lengths don't match.
        if total_len != rhs.len() {
            return false;
        }

        // Check that the chunks of `lhs` is equal to the granule-sized chunks of `rhs`.
        lhs_chunks
            .zip(rhs.as_bytes().chunks(VarLenGranule::DATA_SIZE))
            .all(|(l, r)| l == r)
    }
}

/// Equates `lhs`, assumed to be typed at `T`, to `rhs`.
///
/// SAFETY: Let `lhs = &ctx.lhs.bytes[range_move(0..size_of::<T>(), ctx.curr_offset)]`.
/// Then `lhs` must point to a valid `T` and must be properly aligned for `T`.
unsafe fn eq_at<T: Copy + Eq>(ctx: &mut EqCtx<'_>, rhs: &T) -> bool {
    &unsafe { read_from_bytes::<T>(ctx.lhs.bytes, &mut ctx.curr_offset) } == rhs
}

#[cfg(test)]
mod tests {
    use crate::{blob_store::HashMapBlobStore, page_pool::PagePool};
    use proptest::prelude::*;
    use spacetimedb_sats::proptest::generate_typed_row;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(if cfg!(miri) { 8 } else { 2048 }))]
        #[test]
        fn pv_row_ref_eq((ty, val) in generate_typed_row()) {
            // Turn `val` into a `RowRef`.
            let mut table = crate::table::test::table(ty);
            let blob_store = &mut HashMapBlobStore::default();
            let (_, row) = table.insert(&PagePool::new_for_test(), blob_store, &val).unwrap();

            // Check eq algo.
            prop_assert_eq!(row, val);
        }
    }
}
