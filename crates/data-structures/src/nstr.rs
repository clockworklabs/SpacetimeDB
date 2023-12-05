//! Provides UTF-8 strings of compile-time-known lengths.
//!
//! The strings can be constructed with `nstr!(string_literal)`
//! producing a type `NStr<N>` where `const N: usize`.
//! An example would be `nstr!("spacetime"): NStr<9>`.

use core::{fmt, ops::Deref, str};
use std::ops::DerefMut;

/// A UTF-8 string of known length `N`.
///
/// The notion of length is that of the standard library's `String` type.
/// That is, the length is in bytes, not chars or graphemes.
///
/// It holds that `size_of::<NStr<N>>() == N`
/// but `&NStr<N>` is always word sized.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NStr<const N: usize>([u8; N]);

// This function exists due to macro-privacy interactions.
#[doc(hidden)]
pub const fn __nstr<const N: usize>(s: &str) -> NStr<N> {
    // Ensure that the string was `N` in length.
    // This has no runtime cost as the `nstr!` macro forces compile-time eval.
    if N != s.len() {
        panic!("string does not match claimed length");
    };

    // Convert the string to bytes.
    // Need to use `while` to do this at compile time.
    let src = s.as_bytes();
    let mut dst = [0; N];
    let mut i = 0;
    while i < N {
        dst[i] = src[i];
        i += 1;
    }

    NStr(dst)
}

/// Constructs an `NStr<N>` given a string literal of `N` bytes in length.
///
/// # Example
///
/// ```
/// use spacetimedb_data_structures::{nstr, nstr::NStr};
/// let s: NStr<3> = nstr!("foo");
/// assert_eq!(&*s, "foo");
/// assert_eq!(s.len(), 3);
/// assert_eq!(format!("{s}"), "foo");
/// assert_eq!(format!("{s:?}"), "foo");
/// ```
#[macro_export]
macro_rules! nstr {
    ($lit:literal) => {
        $crate::nstr::__nstr::<{ $lit.len() }>($lit).into()
    };
}

impl<const N: usize> Deref for NStr<N> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: An `NStr<N>` can only be made through `__nstr(..)`
        // and which receives an `&str` which is valid UTF-8.
        unsafe { str::from_utf8_unchecked(&self.0) }
    }
}

impl<const N: usize> DerefMut for NStr<N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: An `NStr<N>` can only be made through `__nstr(..)`
        // and which receives an `&str` which is valid UTF-8.
        unsafe { str::from_utf8_unchecked_mut(&mut self.0) }
    }
}

impl<const N: usize> fmt::Debug for NStr<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.deref())
    }
}

impl<const N: usize> fmt::Display for NStr<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.deref())
    }
}
