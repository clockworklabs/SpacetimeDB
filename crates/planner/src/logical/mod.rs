pub mod bind;
pub mod errors;
pub mod expr;
pub mod stmt;
pub mod ty;

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
