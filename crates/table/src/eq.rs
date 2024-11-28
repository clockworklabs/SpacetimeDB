//! Provides the function [`eq_row_in_page(page_a, page_b, offset_a, offset_b, ty)`]
//! which, for `value_a/b = page_a/b.get_row_data(offset_a/b, fixed_row_size)` typed at `ty`,
//! compares `value_a` and `value_b` for equality.

use super::{
    bflatn_from::read_tag,
    indexes::{Bytes, PageOffset},
    layout::{align_to, AlgebraicTypeLayout, HasLayout, ProductTypeLayout, RowTypeLayout},
    page::Page,
    row_hash::read_from_bytes,
    util::range_move,
    var_len::VarLenRef,
};

/// Equates row `a` in `page_a` with its fixed part starting at `fixed_offset_a`
/// to row `b` in `page_b` with its fixed part starting at `fixed_offset_b`.
/// Both rows last for `ty.size()` bytes
/// and are assumed to be typed at `ty` and must be valid for `ty`.
///
/// Returns whether row `a` is equal to row `b` including their var-len objects.
///
/// # Safety
///
/// 1. `fixed_offset_a/b` are valid offsets for rows typed at `ty` in `page_a/b`.
/// 2. for any `vlr_a/b: VarLenRef` in the fixed parts of row `a` and `b`,
///   `vlr_a/b.first_offset` must either be `NULL` or point to a valid granule in `page_a/b`.
pub unsafe fn eq_row_in_page(
    page_a: &Page,
    page_b: &Page,
    fixed_offset_a: PageOffset,
    fixed_offset_b: PageOffset,
    ty: &RowTypeLayout,
    only_fixed_parts: bool,
) -> bool {
    // Contexts for rows `a` and `b`.
    let a = BytesPage::new(page_a, fixed_offset_a, ty);
    let b = BytesPage::new(page_b, fixed_offset_b, ty);

    // If there are only fixed parts in the layout,
    // there are no pointers to anywhere,
    // So it is sound to simply check for byte-wise equality
    // and we need not do the tree traversal at all.
    if only_fixed_parts {
        return a.bytes == b.bytes;
    }

    // Context for the whole comparison.
    let mut ctx = EqCtx { a, b, curr_offset: 0 };

    // Test for equality!
    // SAFETY:
    // 1. Per requirement 1., rows `a/b` are valid at type `ty` and properly aligned for `ty`.
    //    Their fixed parts are defined as:
    //    `value_a/b = ctx.a/b.bytes[range_move(0..fixed_row_size, fixed_offset_a/b)]`
    //    as needed.
    // 2. for any `vlr_a/b: VarLenRef` stored in `value_a/b`,
    //   `vlr_a/b.first_offset` must either be `NULL` or point to a valid granule in `page_a/b`.
    unsafe { eq_product(&mut ctx, ty.product()) }
}

/// A view into the fixed part of a row combined with the page it belongs to.
#[derive(Clone, Copy)]
pub(crate) struct BytesPage<'page> {
    /// The `Bytes` of the fixed part of a row in `page`.
    pub(crate) bytes: &'page Bytes,
    /// The `Page` which has the fixed part `bytes` and associated var-len objects.
    pub(crate) page: &'page Page,
}

impl<'page> BytesPage<'page> {
    /// Returns a view into the bytes of the row at `offset` in `page` typed at `ty`.
    pub(crate) fn new(page: &'page Page, offset: PageOffset, ty: &RowTypeLayout) -> Self {
        let bytes = page.get_row_data(offset, ty.size());
        Self { page, bytes }
    }
}

/// Comparison context used in the functions below.
#[derive(Clone, Copy)]
struct EqCtx<'page_a, 'page_b> {
    /// The view into the fixed part of row `a` in page `A` to compare against `b`.
    a: BytesPage<'page_a>,
    /// The view into the fixed part of row `b` in page `B` to compare against `a`.
    b: BytesPage<'page_b>,
    /// The current offset at which some sub-object of both `a` and `b` exist.
    curr_offset: usize,
}

/// For every product field in `value_a/b = &ctx.a/b.bytes[range_move(0..ty.size(), *ctx.curr_offset)]`,
/// which is typed at `ty`,
/// equates `value_a/value_b`, including any var-len object,
/// and advance the `ctx.curr_offset`.
///
/// SAFETY:
/// 1. `value_a/b` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr_a/b: VarLenRef` stored in `value_a/b`,
///   `vlr_a/b.first_offset` must either be `NULL` or point to a valid granule in `page_a/b`.
unsafe fn eq_product(ctx: &mut EqCtx<'_, '_>, ty: &ProductTypeLayout) -> bool {
    let base_offset = ctx.curr_offset;
    ty.elements.iter().all(|elem_ty| {
        ctx.curr_offset = base_offset + elem_ty.offset as usize;

        // SAFETY: By 1., `value_a/b` are valid at `ty`,
        // so it follows that valid and properly aligned sub-`value_a/b`s
        // are valid `elem_ty.ty`s.
        // By 2., and the above, it follows that sub-`value_a/b`s won't have dangling `VarLenRef`s.
        unsafe { eq_value(ctx, &elem_ty.ty) }
    })
}

/// For `value_a/b = &ctx.a/b.bytes[range_move(0..ty.size(), *ctx.curr_offset)]` typed at `ty`,
/// equates `value_a == value_b`, including any var-len objects,
/// and advances the `ctx.curr_offset`.
///
/// SAFETY:
/// 1. `value_a/b` must both be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr_a/b: VarLenRef` stored in `value_a/b`,
///   `vlr_a/b.first_offset` must either be `NULL` or point to a valid granule in `page_a/b`.
unsafe fn eq_value(ctx: &mut EqCtx<'_, '_>, ty: &AlgebraicTypeLayout) -> bool {
    debug_assert_eq!(
        ctx.curr_offset,
        align_to(ctx.curr_offset, ty.align()),
        "curr_offset {} insufficiently aligned for type {:?}",
        ctx.curr_offset,
        ty
    );

    match ty {
        AlgebraicTypeLayout::Sum(ty) => {
            // Read the tags of the sum values.
            let (tag_a, data_ty) = read_tag(ctx.a.bytes, ty, ctx.curr_offset);
            let (tag_b, _) = read_tag(ctx.b.bytes, ty, ctx.curr_offset);

            // The tags must match!
            if tag_a != tag_b {
                return false;
            }

            // Equate the variant data values.
            let curr_offset = ctx.curr_offset + ty.offset_of_variant_data(tag_a);
            ctx.curr_offset += ty.size();
            let mut ctx = EqCtx { curr_offset, ..*ctx };
            // SAFETY: `value_a/b` are valid at `ty` so given `tag`,
            // we know `data_value_a/b = &ctx.a/b.bytes[range_move(0..data_ty.size(), data_offset))`
            // are valid at `data_ty`.
            // By 2., and the above, we also know that `data_value_a/b` won't have dangling `VarLenRef`s.
            unsafe { eq_value(&mut ctx, data_ty) }
        }
        AlgebraicTypeLayout::Product(ty) => {
            // SAFETY: `value_a/b` are valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { eq_product(ctx, ty) }
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
        | &AlgebraicTypeLayout::I256
        | &AlgebraicTypeLayout::U256
        | &AlgebraicTypeLayout::F32
        | &AlgebraicTypeLayout::F64 => eq_byte_array(ctx, ty.size()),

        // The var-len cases.
        &AlgebraicTypeLayout::String | AlgebraicTypeLayout::VarLen(_) => {
            // SAFETY: `value_a/b` were valid at and aligned for `ty`.
            // These `ty` each store a `vlr_a/vlr_b: VarLenRef` as their value,
            // so the range is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr_a/vlr_b.first_granule` were promised by the caller
            // to either be `NULL` or point to a valid granule in `page_a/page_b`.
            unsafe { eq_vlo(ctx) }
        }
    }
}

/// Equates the bytes of two var-len objects
/// referred to at by the var-len references in `ctx.a/b.bytes` at `ctx.curr_offset`
/// which is then advanced.
///
/// The function does not care about large-blob-ness.
/// Rather, the blob hash is implicitly tested for equality.
///
/// SAFETY: `data_a/b = ctx.a/b.bytes[range_move(0..size_of::<VarLenRef>(), *ctx.curr_offset)]`
/// must be valid `vlr_a/b = VarLenRef` and `&data_a/b` must be properly aligned for a `VarLenRef`.
/// The `vlr_a/b.first_granule`s must be `NULL` or must point to a valid granule in `ctx.a/b.page`.
unsafe fn eq_vlo(ctx: &mut EqCtx<'_, '_>) -> bool {
    // SAFETY: We have a valid `VarLenRef` at `&data_a`.
    let vlr_a = unsafe { read_from_bytes::<VarLenRef>(ctx.a.bytes, &mut { ctx.curr_offset }) };
    // SAFETY: We have a valid `VarLenRef` at `&data_b`.
    let vlr_b = unsafe { read_from_bytes::<VarLenRef>(ctx.b.bytes, &mut ctx.curr_offset) };

    // Lengths have to match or they cannot be equal.
    // This also implicitly checks that both sides are blobs or neither are.
    if vlr_a.length_in_bytes != vlr_b.length_in_bytes {
        return false;
    }

    // SAFETY: ^-- got valid `VarLenRef` where `vlr_a.first_granule` was `NULL`
    // or a pointer to a valid starting granule, as required.
    let var_iter_a = unsafe { ctx.a.page.iter_vlo_data(vlr_a.first_granule) };
    // SAFETY: ^-- got valid `VarLenRef` where `vlr_b.first_granule` was `NULL`
    // or a pointer to a valid starting granule, as required.
    let var_iter_b = unsafe { ctx.b.page.iter_vlo_data(vlr_b.first_granule) };
    var_iter_a.zip(var_iter_b).all(|(da, db)| da == db)
}

/// Equates the byte arrays `data_a/data_b = ctx.a/b.bytes[range_move(0..len, ctx.curr_offset)]`
/// and advances the offset.
fn eq_byte_array(ctx: &mut EqCtx<'_, '_>, len: usize) -> bool {
    let data_a = &ctx.a.bytes[range_move(0..len, ctx.curr_offset)];
    let data_b = &ctx.b.bytes[range_move(0..len, ctx.curr_offset)];
    ctx.curr_offset += len;
    data_a == data_b
}
