use core::ops::Range;

/// Translates the range `r` by adding `by` to both its `start` and its `end`.
///
/// The resulting range will have the same length as `r`.
pub const fn range_move(r: Range<usize>, by: usize) -> Range<usize> {
    (r.start + by)..(r.end + by)
}

/// Asserts that `$ty` is `$size` bytes in `static_assert_size($ty, $size)`.
///
/// Example:
///
/// ```ignore
/// static_assert_size!(u32, 4);
/// ```
#[macro_export]
macro_rules! static_assert_size {
    ($ty:ty, $size:expr) => {
        const _: [(); $size] = [(); ::core::mem::size_of::<$ty>()];
    };
}

/// Asserts that `$ty` is aligned at `$align` bytes in `static_assert_align($ty, $align)`.
///
/// Example:
///
/// ```ignore
/// static_assert_align!(u32, 2);
/// ```
#[macro_export]
macro_rules! static_assert_align {
    ($ty:ty, $align:expr) => {
        const _: [(); $align] = [(); ::core::mem::align_of::<$ty>()];
    };
}
