use core::mem::{self, MaybeUninit};
use core::ops::Range;

/// Adds `by` to the range `r`.
pub const fn range_add(r: Range<usize>, by: usize) -> Range<usize> {
    (r.start + by)..(r.end + by)
}

/// charwise copy of `MaybeUninit::write_slice`, but that's not stabilized.
/// https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.write_slice
/// Unlike that function, this does not return a reference to the initialized bytes.
pub fn maybe_uninit_write_slice<T: Copy>(this: &mut [MaybeUninit<T>], src: &[T]) {
    // SAFETY: &[T] and &[MaybeUninit<T>] have the same layout
    let uninit_src: &[MaybeUninit<T>] = unsafe { mem::transmute(src) };

    this[0..uninit_src.len()].copy_from_slice(uninit_src);
}

/// charwise copy of `MaybeUninit::slice_assume_init_ref`, but that's not stabilized.
/// https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.slice_assume_init_ref
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

/// charwise copy of `MaybeUninit::uninit_array`, but that's not stabilized.
/// https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.uninit_array
#[allow(unused)] // used in tests
pub const fn uninit_array<T, const N: usize>() -> [MaybeUninit<T>; N] {
    // SAFETY: An uninitialized `[MaybeUninit<_>; LEN]` is valid.
    unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() }
}
