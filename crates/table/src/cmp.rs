//! Provides the function [`cmp_row_in_page(page_a, page_b, offset_a, offset_b, ty)`]
//! which, for `value_a/b = page_a/b.get_row_data(offset_a/b, fixed_row_size)` typed at `ty`,
//! compares `value_a` and `value_b`.
//!
//! String comparison uses lexicographic ordering of the bytes in the utf-8 encoding of the strings.

use super::{
    bflatn_from::read_tag,
    eq::BinCtx,
    indexes::{Bytes, PageOffset},
    layout::{align_to, AlgebraicTypeLayout, HasLayout, ProductTypeLayout, RowTypeLayout},
    page::Page,
    row_hash::read_from_bytes,
    var_len::VarLenRef,
};
use core::cmp::Ordering;

/// Compares row `a` in `page_a` with its fixed part starting at `fixed_offset_a`
/// to row `b` in `page_b` with its fixed part starting at `fixed_offset_b`.
/// Both rows last for `ty.size()` bytes
/// and are assumed to be typed at `ty` and must be valid for `ty`.
///
/// Returns an ordering between row `a` and row `b` including their var-len objects.
///
/// # Safety
///
/// 1. `fixed_offset_a/b` are valid offsets for rows typed at `ty` in `page_a/b`.
/// 2. for any `vlr_a/b: VarLenRef` in the fixed parts of row `a` and `b`,
///   `vlr_a/b.first_offset` must either be `NULL` or point to a valid granule in `page_a/b`.
pub unsafe fn cmp_row_in_page(
    page_a: &Page,
    page_b: &Page,
    fixed_offset_a: PageOffset,
    fixed_offset_b: PageOffset,
    ty: &RowTypeLayout,
) -> Ordering {
    // Context for the whole comparison.
    let mut ctx = BinCtx::new(ty, page_a, page_b, fixed_offset_a, fixed_offset_b);
    // Test for equality!
    // SAFETY:
    // 1. Per requirement 1., rows `a/b` are valid at type `ty` and properly aligned for `ty`.
    //    Their fixed parts are defined as:
    //    `value_a/b = ctx.a/b.bytes[range_move(0..fixed_row_size, fixed_offset_a/b)]`
    //    as needed.
    // 2. for any `vlr_a/b: VarLenRef` stored in `value_a/b`,
    //   `vlr_a/b.first_offset` must either be `NULL` or point to a valid granule in `page_a/b`.
    unsafe { cmp_product(&mut ctx, ty.product()) }
}

/// For every product field in `value_a/b = &ctx.a/b.bytes[range_move(0..ty.size(), *ctx.curr_offset)]`,
/// which is typed at `ty`,
/// compares `value_a/value_b`, including any var-len object,
/// and advance the `ctx.curr_offset`.
///
/// SAFETY:
/// 1. `value_a/b` must be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr_a/b: VarLenRef` stored in `value_a/b`,
///   `vlr_a/b.first_offset` must either be `NULL` or point to a valid granule in `page_a/b`.
unsafe fn cmp_product(ctx: &mut BinCtx<'_, '_>, ty: &ProductTypeLayout) -> Ordering {
    for field_ty in ty.elements.iter() {
        // SAFETY: By 1., `value_a/b` are valid at `ty`,
        // so it follows that valid and properly aligned sub-`value_a/b`s
        // are valid `elem_ty.ty`s.
        // By 2., and the above, it follows that sub-`value_a/b`s won't have dangling `VarLenRef`s.
        match unsafe { cmp_value(ctx, &field_ty.ty) } {
            Ordering::Equal => {}
            ord => return ord,
        }
    }
    Ordering::Equal
}

/// For `value_a/b = &ctx.a/b.bytes[range_move(0..ty.size(), *ctx.curr_offset)]` typed at `ty`,
/// compares `value_a/value_b`, including any var-len objects,
/// and advances the `ctx.curr_offset`.
///
/// SAFETY:
/// 1. `value_a/b` must both be valid at type `ty` and properly aligned for `ty`.
/// 2. for any `vlr_a/b: VarLenRef` stored in `value_a/b`,
///   `vlr_a/b.first_offset` must either be `NULL` or point to a valid granule in `page_a/b`.
unsafe fn cmp_value(ctx: &mut BinCtx<'_, '_>, ty: &AlgebraicTypeLayout) -> Ordering {
    let ty_alignment = ty.align();
    ctx.curr_offset = align_to(ctx.curr_offset, ty_alignment);

    let res = match ty {
        AlgebraicTypeLayout::Sum(ty) => {
            // Read the tags of the sum values.
            // SAFETY: `ctx.a.bytes[curr_offset..]` hold a sum value at `ty`.
            let (tag_a, data_ty) = unsafe { read_tag(ctx.a.bytes, ty, ctx.curr_offset) };
            // SAFETY: `ctx.b.bytes[curr_offset..]` hold a sum value at `ty`.
            let (tag_b, _) = unsafe { read_tag(ctx.b.bytes, ty, ctx.curr_offset) };

            // The tags must match!
            if tag_a != tag_b {
                // Sure hope the variants were declared in the correct order!
                return tag_a.cmp(&tag_b);
            }

            // Equate the variant data values.
            let curr_offset = ctx.curr_offset + ty.offset_of_variant_data(tag_a);
            ctx.curr_offset += ty.size();
            let mut ctx = BinCtx { curr_offset, ..*ctx };
            // SAFETY: `value_a/b` are valid at `ty` so given `tag`,
            // we know `data_value_a/b = &ctx.a/b.bytes[range_move(0..data_ty.size(), data_offset))`
            // are valid at `data_ty`.
            // By 2., and the above, we also know that `data_value_a/b` won't have dangling `VarLenRef`s.
            unsafe { cmp_value(&mut ctx, data_ty) }
        }
        AlgebraicTypeLayout::Product(ty) => {
            // SAFETY: `value_a/b` are valid at `ty` and `VarLenRef`s won't be dangling.
            unsafe { cmp_product(ctx, ty) }
        }

        // The primitive types:
        &AlgebraicTypeLayout::Bool => unsafe { cmp_primitive::<bool>(ctx) },
        &AlgebraicTypeLayout::I8 => unsafe { cmp_primitive::<i8>(ctx) },
        &AlgebraicTypeLayout::U8 => unsafe { cmp_primitive::<u8>(ctx) },
        &AlgebraicTypeLayout::I16 => unsafe { cmp_primitive::<i16>(ctx) },
        &AlgebraicTypeLayout::U16 => unsafe { cmp_primitive::<u16>(ctx) },
        &AlgebraicTypeLayout::I32 => unsafe { cmp_primitive::<i32>(ctx) },
        &AlgebraicTypeLayout::U32 => unsafe { cmp_primitive::<u32>(ctx) },
        &AlgebraicTypeLayout::I64 => unsafe { cmp_primitive::<i64>(ctx) },
        &AlgebraicTypeLayout::U64 => unsafe { cmp_primitive::<u64>(ctx) },
        &AlgebraicTypeLayout::I128 => unsafe { cmp_primitive::<i128>(ctx) },
        &AlgebraicTypeLayout::U128 => unsafe { cmp_primitive::<u128>(ctx) },
        &AlgebraicTypeLayout::F32 => unsafe { cmp_primitive_with_conversion::<f32, decorum::Total<f32>>(ctx) },
        &AlgebraicTypeLayout::F64 => unsafe { cmp_primitive_with_conversion::<f64, decorum::Total<f64>>(ctx) },

        // The var-len cases.
        &AlgebraicTypeLayout::String | AlgebraicTypeLayout::VarLen(_) => {
            // SAFETY: `value_a/b` were valid at and aligned for `ty`.
            // These `ty` each store a `vlr_a/vlr_b: VarLenRef` as their value,
            // so the range is valid and properly aligned for `VarLenRef`.
            // Moreover, `vlr_a/vlr_b.first_granule` were promised by the caller
            // to either be `NULL` or point to a valid granule in `page_a/page_b`.
            unsafe { cmp_vlo(ctx) }
        }
    };
    // TODO(perf,bikeshedding): unncessary work for some cases?
    ctx.curr_offset = align_to(ctx.curr_offset, ty_alignment);
    res
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
unsafe fn cmp_vlo(ctx: &mut BinCtx<'_, '_>) -> Ordering {
    // SAFETY: We have a valid `VarLenRef` at `&data_a`.
    let vlr_a = unsafe { read_from_bytes::<VarLenRef>(ctx.a.bytes, &mut { ctx.curr_offset }) };
    // SAFETY: We have a valid `VarLenRef` at `&data_b`.
    let vlr_b = unsafe { read_from_bytes::<VarLenRef>(ctx.b.bytes, &mut ctx.curr_offset) };

    // SAFETY: ^-- got valid `VarLenRef` where `vlr_a.first_granule` was `NULL`
    // or a pointer to a valid starting granule, as required.
    let mut var_iter_a = unsafe { ctx.a.page.iter_vlo_data(vlr_a.first_granule) };
    // SAFETY: ^-- got valid `VarLenRef` where `vlr_b.first_granule` was `NULL`
    // or a pointer to a valid starting granule, as required.
    let mut var_iter_b = unsafe { ctx.b.page.iter_vlo_data(vlr_b.first_granule) };
    loop {
        match (var_iter_a.next(), var_iter_b.next()) {
            (Some(byte_a), Some(byte_b)) => match byte_a.cmp(byte_b) {
                Ordering::Equal => {}
                ord => return ord,
            },
            (None, None) => return Ordering::Equal,
            // chosen arbitrarily
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
}

/// Compares the primitive values `value_a/b = ctx.a/b.bytes[range_move(0..ty.size(), *ctx.curr_offset)]`
/// and advances the offset.
///
/// SAFETY: `value_a/b` must be valid at type `ty` and properly aligned for `ty`.
unsafe fn cmp_primitive<T: Sized + Ord + Copy>(ctx: &mut BinCtx<'_, '_>) -> Ordering {
    let value_a = unsafe { read_primitive::<T>(ctx.a.bytes, ctx.curr_offset) };
    let value_b = unsafe { read_primitive::<T>(ctx.b.bytes, ctx.curr_offset) };
    ctx.curr_offset += std::mem::size_of::<T>();
    value_a.cmp(&value_b)
}

/// Compares the primitive values `value_a/b = ctx.a/b.bytes[range_move(0..ty.size(), *ctx.curr_offset)]`
/// after converting them to `Total` and advances the offset.
///
/// SAFETY: `value_a/b` must be valid at type `ty` and properly aligned for `ty`.
unsafe fn cmp_primitive_with_conversion<Partial: Sized + Into<Total> + Copy, Total: Sized + Ord>(
    ctx: &mut BinCtx<'_, '_>,
) -> Ordering {
    let value_a: Total = unsafe { read_primitive::<Partial>(ctx.a.bytes, ctx.curr_offset).into() };
    let value_b: Total = unsafe { read_primitive::<Partial>(ctx.b.bytes, ctx.curr_offset).into() };
    ctx.curr_offset += std::mem::size_of::<Partial>();
    value_a.cmp(&value_b)
}

/// SAFETY: `bytes[range_move(0..ty.size(), offset)]` must be valid at type `ty` and properly aligned for `ty`.
unsafe fn read_primitive<T: Sized + Copy>(bytes: &Bytes, offset: usize) -> T {
    unsafe {
        let ptr = bytes.as_ptr().add(offset) as *const T;
        *ptr
    }
}
