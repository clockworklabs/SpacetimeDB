use core::mem::{self, MaybeUninit};
use core::ops::Range;

/// Translates the range `r` by adding `by` to both its `start` and its `end`.
///
/// The resulting range will have the same length as `r`.
pub const fn range_move(r: Range<usize>, by: usize) -> Range<usize> {
    (r.start + by)..(r.end + by)
}

/// Copy elements from `src` into `this`, initializing those elements of `this`.
///
/// If `this` is longer than `src`, write only the first `src.len()` elements of `this`.
///
/// If `src` is longer than `this`, panic.
///
/// Copy of the source of `MaybeUninit::write_slice`, but that's not stabilized.
/// https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.write_slice
/// Unlike that function, this does not return a reference to the initialized bytes.
pub fn maybe_uninit_write_slice<T: Copy>(this: &mut [MaybeUninit<T>], src: &[T]) {
    // SAFETY: &[T] and &[MaybeUninit<T>] have the same layout
    let uninit_src: &[MaybeUninit<T>] = unsafe { mem::transmute(src) };

    this[0..uninit_src.len()].copy_from_slice(uninit_src);
}

/// Convert a `[MaybeUninit<T>]` into a `[T]` by asserting all elements are initialized.
///
/// Identitcal copy of the source of `MaybeUninit::slice_assume_init_ref`, but that's not stabilized.
/// https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.slice_assume_init_ref
///
/// SAFETY: all elements of `slice` must be initialized.
pub const unsafe fn slice_assume_init_ref<T>(slice: &[MaybeUninit<T>]) -> &[T] {
    // SAFETY: casting `slice` to a `*const [T]` is safe since the caller guarantees that
    // `slice` is initialized, and `MaybeUninit` is guaranteed to have the same layout as `T`.
    // The pointer obtained is valid since it refers to memory owned by `slice` which is a
    // reference and thus guaranteed to be valid for reads.
    unsafe { &*(slice as *const [MaybeUninit<T>] as *const [T]) }
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

/// Construct an uninitialized array of `N` elements.
///
/// The array will be appropriately sized and aligned to hold `N` elements of type `T`,
/// but those elements will be uninitialized.
///
/// Identitcal copy of the source of `MaybeUninit::uninit_array`, but that's not stabilized.
/// https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.uninit_array
pub const fn uninit_array<T, const N: usize>() -> [MaybeUninit<T>; N] {
    // SAFETY: An uninitialized `[MaybeUninit<_>; N]` is valid.
    unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() }
}
