use core::{
    borrow::Borrow,
    cmp::Ordering,
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    ptr::{slice_from_raw_parts_mut, NonNull},
    slice,
    str::{from_utf8_unchecked, from_utf8_unchecked_mut},
};
use thiserror::Error;

// =============================================================================
// Errors
// =============================================================================

#[derive(Error, Debug)]
#[error("The length `{len}` was too long")]
/// An error signifying that a container's size was over `u32::MAX`.
pub struct LenTooLong<T = ()> {
    /// The size of the container that was too large.
    pub len: usize,
    /// The container that was too large for into-slim conversion.
    pub too_long: T,
}

impl<T> LenTooLong<T> {
    /// Forgets the container part of the error.
    pub fn forget(self) -> LenTooLong {
        self.map(drop)
    }

    /// Maps the container part of the error.
    pub fn map<U>(self, with: impl FnOnce(T) -> U) -> LenTooLong<U> {
        LenTooLong {
            len: self.len,
            too_long: with(self.too_long),
        }
    }
}

/// Try to convert `x` into `B` and forget the container part of the error if any.
pub fn try_into<A, B: TryFrom<A, Error = LenTooLong<A>>>(x: A) -> Result<B, LenTooLong> {
    x.try_into().map_err(|e: LenTooLong<A>| e.forget())
}

impl<A> crate::ser::Error for LenTooLong<A> {
    fn custom<T: Display>(_: T) -> Self {
        unimplemented!()
    }
}

// =============================================================================
// Utils
// =============================================================================

/// Ensures that `$thing.len() <= u32::MAX`.
macro_rules! ensure_len_fits {
    ($thing:expr) => {
        let Ok(_) = u32::try_from($thing.len()) else {
            return Err(LenTooLong {
                len: $thing.len(),
                too_long: $thing,
            });
        };
    };
}

/// Convert to `A` but panic if `x.len() > u32::MAX`.
fn expect_fit<A, E, B: TryInto<A, Error = LenTooLong<E>>>(x: B) -> A {
    x.try_into().map_err(|e| e.len).expect("length didn't fit in `u32`")
}

fn into_box<T, U: Into<Box<[T]>>>(x: U) -> Box<[T]> {
    x.into()
}

// =============================================================================
// Raw slice utility type
// =============================================================================

/// Implementation detail of the other types.
/// Provides some convenience but users of the type are responsible
/// for safety, invariants and variance.
#[repr(packed)]
struct SlimRawSlice<T> {
    /// A valid pointer to the slice data.
    ptr: NonNull<T>,
    /// The length of the slice.
    len: u32,
}

impl<T> SlimRawSlice<T> {
    /// Split `self` into a raw pointer and the slice length.
    fn split(self) -> (*mut T, usize) {
        (self.ptr.as_ptr(), self.len as usize)
    }

    /// Dereferences this raw slice into a shared slice.
    ///
    /// SAFETY: `self.ptr` and `self.len`
    /// must satisfy [`std::slice::from_raw_parts`]'s requirements.
    /// That is,
    /// * `self.ptr` must be valid for reads
    ///    for `self.len * size_of::<T>` many bytes and must be aligned.
    ///
    /// * `self.ptr` must point to `self.len`
    ///    consecutive properly initialized values of type `T`.
    ///
    /// * The memory referenced by the returned slice
    ///   must not be mutated for the duration of lifetime `'a`,
    ///   except inside an `UnsafeCell`.
    ///
    /// * The total size `self.len * mem::size_of::<T>()`
    ///   of the slice must be no larger than `isize::MAX`,
    ///   and adding that size to `data`
    ///   must not "wrap around" the address space.
    #[allow(clippy::needless_lifetimes)]
    unsafe fn deref<'a>(&'a self) -> &'a [T] {
        let (ptr, len) = self.split();
        // SAFETY: caller is responsible for these.
        unsafe { slice::from_raw_parts(ptr, len) }
    }

    /// Dereferences this raw slice into a mutable slice.
    ///
    /// SAFETY: `self.ptr` and `self.len`
    /// must satisfy [`std::slice::from_raw_parts_mut`]'s requirements.
    /// That is,
    /// * `self.ptr` must be [valid] for both reads and writes
    ///    for `self.len * mem::size_of::<T>()` many bytes,
    ///   and it must be properly aligned.
    ///
    /// * `self.ptr` must point to `self.len`
    ///   consecutive properly initialized values of type `T`.
    ///
    /// * The memory referenced by the returned slice
    ///   must not be accessed through any other pointer
    ///   (not derived from the return value) for the duration of lifetime `'a`.
    ///   Both read and write accesses are forbidden.
    ///
    /// * The total size `self.len * mem::size_of::<T>()`
    ///   of the slice must be no larger than `isize::MAX`,
    ///   and adding that size to `data` must not "wrap around" the address space.
    #[allow(clippy::needless_lifetimes)]
    unsafe fn deref_mut<'a>(&'a mut self) -> &'a mut [T] {
        let (ptr, len) = self.split();
        // SAFETY: caller is responsible for these.
        unsafe { slice::from_raw_parts_mut(ptr, len) }
    }

    /// Creates the raw slice from a pointer to the data and a length.
    ///
    /// It is assumed that `len <= u32::MAX`.
    /// The caller must ensure that `ptr != NULL`.
    const unsafe fn from_len_ptr(len: usize, ptr: *mut T) -> Self {
        // SAFETY: caller ensured that `!ptr.is_null()`.
        let ptr = NonNull::new_unchecked(ptr);
        let len = len as u32;
        Self { ptr, len }
    }
}

impl<T> Copy for SlimRawSlice<T> {}
impl<T> Clone for SlimRawSlice<T> {
    fn clone(&self) -> Self {
        *self
    }
}

// =============================================================================
// Owned boxed slice
// =============================================================================

pub use slim_slice_box::*;
mod slim_slice_box {
    // ^-- In the interest of soundness,
    // this module exists to limit access to the private fields
    // of `SlimSliceBox<T>` to a few key functions.
    use super::*;

    /// Provides a slimmer version of `Box<[T]>`
    /// using `u32` for its length instead of `usize`.
    #[repr(transparent)]
    pub struct SlimSliceBox<T> {
        /// The representation of this boxed slice.
        ///
        /// To convert to a `SlimSliceBox<T>` we must first have a `Box<[T]>`.
        raw: SlimRawSlice<T>,
        /// Marker to ensure covariance and dropck ownership.
        owned: PhantomData<T>,
    }

    impl<T> Drop for SlimSliceBox<T> {
        fn drop(&mut self) {
            // Get us an owned `SlimSliceBox<T>`
            // by replacing `self` with garbage that won't be dropped
            // as the drop glue for the constituent fields does nothing.
            let ptr = NonNull::dangling();
            let raw = SlimRawSlice { len: 0, ptr };
            let owned = PhantomData;
            let this = mem::replace(self, Self { raw, owned });

            // Convert into `Box<[T]>` and let it deal with dropping.
            drop(into_box(this));
        }
    }

    impl<T> SlimSliceBox<T> {
        /// Converts `boxed` to `Self` without checking `boxed.len() <= u32::MAX`.
        ///
        /// Only safe to call if the constraint above is satisfied.
        #[allow(clippy::boxed_local)]
        // Clippy doesn't seem to consider unsafe code here.
        pub(super) unsafe fn from_boxed_unchecked(boxed: Box<[T]>) -> Self {
            let len = boxed.len();
            let ptr = Box::into_raw(boxed) as *mut T;
            // SAFETY: `Box<T>`'s ptr was a `NonNull<T>` already.
            // and our caller has promised that `boxed.len() <= u32::MAX`.
            let raw = SlimRawSlice::from_len_ptr(len, ptr);
            let owned = PhantomData;
            Self { raw, owned }
        }

        /// Returns a limited shared slice to this boxed slice.
        #[allow(clippy::needless_lifetimes)]
        pub fn shared_ref<'a>(&'a self) -> &'a SlimSlice<'a, T> {
            // SAFETY: The reference lives as long as `self`.
            // By virtue of `repr(transparent)` we're also allowed these reference casts.
            unsafe { mem::transmute(self) }
        }

        /// Returns a limited mutable slice to this boxed slice.
        #[allow(clippy::needless_lifetimes)]
        pub fn exclusive_ref<'a>(&'a mut self) -> &'a mut SlimSliceMut<'a, T> {
            // SAFETY: The reference lives as long as `self`
            // and we have exclusive access to the heap data thanks to `&'a mut self`.
            // By virtue of `repr(transparent)` we're also allowed these reference casts.
            unsafe { mem::transmute(self) }
        }
    }

    impl<T> From<SlimSliceBox<T>> for Box<[T]> {
        fn from(slice: SlimSliceBox<T>) -> Self {
            let slice = ManuallyDrop::new(slice);
            let (ptr, len) = slice.raw.split();
            // SAFETY: All paths to creating a `SlimSliceBox`
            // go through `SlimSliceBox::from_boxed_unchecked`
            // which requires a valid `Box<[T]>`.
            // The function also uses `Box::into_raw`
            // and the original length is kept.
            //
            // It therefore follows that if we reuse the same
            // pointer and length as given to us by a valid `Box<[T]>`,
            // we can use `Box::from_raw` to reconstruct the `Box<[T]>`.
            //
            // We also no longer claim ownership of the data pointed to by `ptr`
            // by virtue of `ManuallyDrop` preventing `Drop for SlimSliceBox<T>`.
            unsafe { Box::from_raw(slice_from_raw_parts_mut(ptr, len)) }
        }
    }
}

/// `SlimSliceBox<T>` is `Send` if `T` is `Send` because the data is owned.
unsafe impl<T: Send> Send for SlimSliceBox<T> {}

/// `SlimSliceBox<T>` is `Sync` if `T` is `Sync` because the data is owned.
unsafe impl<T: Sync> Sync for SlimSliceBox<T> {}

impl<T> Deref for SlimSliceBox<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.shared_ref().deref()
    }
}

impl<T> DerefMut for SlimSliceBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.exclusive_ref().deref_mut()
    }
}

impl<T> SlimSliceBox<T> {
    /// Converts `boxed: Box<[T]>` into `SlimSliceBox<T>`.
    ///
    /// Panics when `boxed.len() > u32::MAX`.
    pub fn from_boxed(boxed: Box<[T]>) -> Self {
        expect_fit(boxed)
    }

    /// Converts `vec: Vec<T>` into `SlimSliceBox<T>`.
    ///
    /// Panics when `vec.len() > u32::MAX`.
    pub fn from_vec(vec: Vec<T>) -> Self {
        Self::from_boxed(vec.into())
    }
}

impl<T> TryFrom<Box<[T]>> for SlimSliceBox<T> {
    type Error = LenTooLong<Box<[T]>>;

    fn try_from(boxed: Box<[T]>) -> Result<Self, Self::Error> {
        ensure_len_fits!(boxed);
        // SAFETY: Checked above that `len <= u32::MAX`.
        Ok(unsafe { Self::from_boxed_unchecked(boxed) })
    }
}

impl<T> TryFrom<Vec<T>> for SlimSliceBox<T> {
    type Error = LenTooLong<Vec<T>>;

    fn try_from(vec: Vec<T>) -> Result<Self, Self::Error> {
        ensure_len_fits!(vec);
        // SAFETY: Checked above that `len <= u32::MAX`.
        Ok(unsafe { Self::from_boxed_unchecked(vec.into_boxed_slice()) })
    }
}

impl<T, const N: usize> From<[T; N]> for SlimSliceBox<T> {
    fn from(arr: [T; N]) -> Self {
        // Make sure `N <= u32::MAX`.
        struct AssertU32<const N: usize>;
        impl<const N: usize> AssertU32<N> {
            const OK: () = assert!(N <= u32::MAX as usize);
        }
        #[allow(clippy::let_unit_value)]
        let () = AssertU32::<N>::OK;

        // SAFETY: We verified statically by `AssertU32<N>` above that `N` fits in u32.
        unsafe { Self::from_boxed_unchecked(into_box(arr)) }
    }
}

impl<T> From<SlimSliceBox<T>> for Vec<T> {
    fn from(slice: SlimSliceBox<T>) -> Self {
        into_box(slice).into()
    }
}

impl<T: Debug> Debug for SlimSliceBox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.deref(), f)
    }
}

impl<T: Clone> Clone for SlimSliceBox<T> {
    fn clone(&self) -> Self {
        // Allocate exactly the right amount
        // so we later don't reallocate due to excess capacity.
        let mut vec = Vec::with_capacity(self.len());
        vec.extend_from_slice(self);
        // SAFETY: We know `self.len() <= u32::MAX`.
        unsafe { Self::from_boxed_unchecked(into_box(vec)) }
    }
}

impl<R: Deref, T> PartialEq<R> for SlimSliceBox<T>
where
    [T]: PartialEq<R::Target>,
{
    fn eq(&self, other: &R) -> bool {
        **self == **other
    }
}

impl<T: Eq> Eq for SlimSliceBox<T> {}

impl<R: Deref, T> PartialOrd<R> for SlimSliceBox<T>
where
    [T]: PartialOrd<R::Target>,
{
    fn partial_cmp(&self, other: &R) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord> Ord for SlimSliceBox<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: Hash> Hash for SlimSliceBox<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&**self, state)
    }
}

impl<T> IntoIterator for SlimSliceBox<T> {
    type Item = T;
    type IntoIter = <Vec<T> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        Vec::from(self).into_iter()
    }
}

/// A wrapper to achieve `FromIterator<T> for Result<SlimSliceBox<T>, Vec<T>>`.
///
/// We cannot do this directly due to orphan rules.
pub struct SlimSliceBoxCollected<T> {
    /// The result of `from_iter`.
    pub inner: Result<SlimSliceBox<T>, LenTooLong<Vec<T>>>,
}

impl<T: Debug> SlimSliceBoxCollected<T> {
    pub fn unwrap(self) -> SlimSliceBox<T> {
        self.inner.expect("number of elements overflowed `u32::MAX`")
    }
}

impl<A> FromIterator<A> for SlimSliceBoxCollected<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let inner = iter.into_iter().collect::<Vec<_>>().try_into();
        SlimSliceBoxCollected { inner }
    }
}

// =============================================================================
// String buffer
// =============================================================================

/// Provides a slimmer version of `Box<str>`
/// using `u32` for its length instead of `usize`.
#[repr(transparent)]
pub struct SlimStrBox {
    /// The underlying byte slice.
    raw: SlimSliceBox<u8>,
}

impl SlimStrBox {
    /// Converts `boxed: Box<str>` into `SlimStrBox`.
    ///
    /// Panics when `boxed.len() > u32::MAX`.
    #[inline]
    pub fn from_boxed(boxed: Box<str>) -> Self {
        expect_fit(boxed)
    }

    /// Converts `str: String` into `SlimStrBox`.
    ///
    /// Panics when `str.len() > u32::MAX`.
    #[inline]
    pub fn from_string(str: String) -> Self {
        Self::from_boxed(str.into())
    }

    /// Returns a limited shared string slice to this boxed string slice.
    #[allow(clippy::needless_lifetimes)]
    pub fn shared_ref<'a>(&'a self) -> &'a SlimStr<'a> {
        // SAFETY: The reference lives as long as `self`,
        // we have shared access already,
        // and by construction we know it's UTF-8.
        unsafe { mem::transmute(self.raw.shared_ref()) }
    }

    /// Returns a limited mutable string slice to this boxed string slice.
    #[allow(clippy::needless_lifetimes)]
    pub fn exclusive_ref<'a>(&'a mut self) -> &'a mut SlimStrMut<'a> {
        // SAFETY: The reference lives as long as `self`,
        // we had `&mut self`,
        // and by construction we know it's UTF-8.
        unsafe { mem::transmute(self.raw.exclusive_ref()) }
    }
}

impl Deref for SlimStrBox {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.shared_ref().deref()
    }
}

impl DerefMut for SlimStrBox {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.exclusive_ref().deref_mut()
    }
}

impl TryFrom<Box<str>> for SlimStrBox {
    type Error = LenTooLong<Box<str>>;

    #[inline]
    fn try_from(boxed: Box<str>) -> Result<Self, Self::Error> {
        ensure_len_fits!(boxed);
        // SAFETY: Checked above that `len <= u32::MAX`.
        let raw = unsafe { SlimSliceBox::from_boxed_unchecked(into_box(boxed)) };
        Ok(Self { raw })
    }
}

impl TryFrom<String> for SlimStrBox {
    type Error = LenTooLong<String>;

    #[inline]
    fn try_from(str: String) -> Result<Self, Self::Error> {
        ensure_len_fits!(str);
        // SAFETY: Checked above that `len <= u32::MAX`.
        let raw = unsafe { SlimSliceBox::from_boxed_unchecked(into_box(str.into_boxed_str())) };
        Ok(Self { raw })
    }
}

impl<'a> TryFrom<&'a str> for SlimStrBox {
    type Error = LenTooLong<&'a str>;

    #[inline]
    fn try_from(str: &'a str) -> Result<Self, Self::Error> {
        str.try_into().map(|s: SlimStr<'_>| s.into())
    }
}

impl From<SlimStrBox> for Box<str> {
    fn from(str: SlimStrBox) -> Self {
        let raw_box = into_box(str.raw);
        // SAFETY: By construction, `SlimStrBox` is valid UTF-8.
        unsafe { Box::from_raw(Box::into_raw(raw_box) as *mut str) }
    }
}

impl From<SlimStrBox> for String {
    fn from(str: SlimStrBox) -> Self {
        <Box<str>>::from(str).into()
    }
}

impl Debug for SlimStrBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.deref(), f)
    }
}

impl Display for SlimStrBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.deref(), f)
    }
}

impl Clone for SlimStrBox {
    fn clone(&self) -> Self {
        Self { raw: self.raw.clone() }
    }
}

impl<R: Deref> PartialEq<R> for SlimStrBox
where
    str: PartialEq<R::Target>,
{
    fn eq(&self, other: &R) -> bool {
        self.deref() == other.deref()
    }
}

impl Eq for SlimStrBox {}

impl<R: Deref> PartialOrd<R> for SlimStrBox
where
    str: PartialOrd<R::Target>,
{
    fn partial_cmp(&self, other: &R) -> Option<Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}

impl Ord for SlimStrBox {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other.deref())
    }
}

impl Hash for SlimStrBox {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self.deref(), state)
    }
}

impl Borrow<str> for SlimStrBox {
    fn borrow(&self) -> &str {
        self
    }
}

// =============================================================================
// Shared slice reference
// =============================================================================

#[allow(clippy::module_inception)]
mod slim_slice {
    use super::*;

    /// A shared reference to `[T]` limited to `u32::MAX` in length.
    #[repr(transparent)]
    #[derive(Clone, Copy)]
    pub struct SlimSlice<'a, T> {
        /// The representation of this shared slice.
        raw: SlimRawSlice<T>,
        /// Marker to ensure covariance for `'a`.
        covariant: PhantomData<&'a [T]>,
    }

    impl<'a, T> SlimSlice<'a, T> {
        /// Converts a `&[T]` to the limited version without length checking.
        ///
        /// SAFETY: `slice.len() <= u32::MAX` must hold.
        pub(super) const unsafe fn from_slice_unchecked(slice: &'a [T]) -> Self {
            let len = slice.len();
            let ptr = slice.as_ptr().cast_mut();
            // SAFETY: `&mut [T]` implies that the pointer is non-null.
            let raw = SlimRawSlice::from_len_ptr(len, ptr);
            // SAFETY: Our length invariant is satisfied by the caller.
            let covariant = PhantomData;
            Self { raw, covariant }
        }
    }

    impl<T> Deref for SlimSlice<'_, T> {
        type Target = [T];

        fn deref(&self) -> &Self::Target {
            // SAFETY: `ptr` and `len` are either
            // a) derived from a live `Box<[T]>` valid for `'self`
            // b) derived from a live `&'self [T]`
            // so we satisfy all safety requirements for `from_raw_parts`.
            unsafe { self.raw.deref() }
        }
    }
}
pub use slim_slice::*;

// SAFETY: Same rules as for `&[T]`.
unsafe impl<T: Send + Sync> Send for SlimSlice<'_, T> {}

// SAFETY: Same rules as for `&[T]`.
unsafe impl<T: Sync> Sync for SlimSlice<'_, T> {}

impl<T: Debug> Debug for SlimSlice<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.deref(), f)
    }
}

impl<T: Hash> Hash for SlimSlice<'_, T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self.deref(), state)
    }
}

impl<T: Eq> Eq for SlimSlice<'_, T> {}
impl<T: PartialEq> PartialEq for SlimSlice<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}
impl<T: PartialEq> PartialEq<[T]> for SlimSlice<'_, T> {
    fn eq(&self, other: &[T]) -> bool {
        self.deref() == other
    }
}

impl<T: Ord> Ord for SlimSlice<'_, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other)
    }
}
impl<T: PartialOrd> PartialOrd for SlimSlice<'_, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}
impl<T: PartialOrd> PartialOrd<[T]> for SlimSlice<'_, T> {
    fn partial_cmp(&self, other: &[T]) -> Option<Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<T: Clone> From<&SlimSlice<'_, T>> for SlimSliceBox<T> {
    fn from(slice: &SlimSlice<'_, T>) -> Self {
        let boxed = into_box(slice.deref());
        // SAFETY: `slice` is limited to `len: u32` by construction.
        unsafe { Self::from_boxed_unchecked(boxed) }
    }
}
impl<T: Clone> From<&SlimSlice<'_, T>> for Box<[T]> {
    fn from(slice: &SlimSlice<'_, T>) -> Self {
        slice.deref().into()
    }
}
impl<T: Clone> From<&SlimSlice<'_, T>> for Vec<T> {
    fn from(slice: &SlimSlice<'_, T>) -> Self {
        slice.deref().into()
    }
}

impl<T: Clone> From<SlimSlice<'_, T>> for SlimSliceBox<T> {
    fn from(slice: SlimSlice<'_, T>) -> Self {
        (&slice).into()
    }
}
impl<T: Clone> From<SlimSlice<'_, T>> for Box<[T]> {
    fn from(slice: SlimSlice<'_, T>) -> Self {
        slice.deref().into()
    }
}
impl<T: Clone> From<SlimSlice<'_, T>> for Vec<T> {
    fn from(slice: SlimSlice<'_, T>) -> Self {
        slice.deref().into()
    }
}

impl<'a, T> TryFrom<&'a [T]> for SlimSlice<'a, T> {
    type Error = LenTooLong<&'a [T]>;

    fn try_from(slice: &'a [T]) -> Result<Self, Self::Error> {
        ensure_len_fits!(slice);
        // SAFETY: ^-- satisfies `len <= u32::MAX`.
        Ok(unsafe { Self::from_slice_unchecked(slice) })
    }
}

/// Converts `&[T]` into the slim limited version.
///
/// Panics when `slice.len() > u32::MAX`.
#[inline]
pub fn slice<T>(s: &[T]) -> SlimSlice<'_, T> {
    expect_fit(s)
}

// =============================================================================
// Mutable slice reference
// =============================================================================

/// A mutable reference to `[T]` limited to `u32::MAX` in length.
#[repr(transparent)]
pub struct SlimSliceMut<'a, T> {
    /// The representation of this mutable slice.
    raw: SlimRawSlice<T>,
    /// Marker to ensure invariance for `'a`.
    invariant: PhantomData<&'a mut [T]>,
}

// SAFETY: Same rules as for `&mut [T]`.
unsafe impl<T: Send> Send for SlimSliceMut<'_, T> {}

// SAFETY: Same rules as for `&mut [T]`.
unsafe impl<T: Sync> Sync for SlimSliceMut<'_, T> {}

impl<'a, T> SlimSliceMut<'a, T> {
    /// Convert this mutable reference to a shared one.
    pub fn shared(&'a self) -> &'a SlimSlice<'a, T> {
        // SAFETY: By virtue of `&'a mut X -> &'a X` being sound, this is also.
        // The types and references to them have the same layout as well.
        unsafe { mem::transmute(self) }
    }

    /// Converts a `&mut [T]` to the limited version without length checking.
    ///
    /// SAFETY: `slice.len() <= u32::MAX` must hold.
    unsafe fn from_slice_unchecked(slice: &'a mut [T]) -> Self {
        // SAFETY: `&mut [T]` implies that the pointer is non-null.
        let raw = SlimRawSlice::from_len_ptr(slice.len(), slice.as_mut_ptr());
        // SAFETY: Our invariants are satisfied by the caller
        // and that `&mut [T]` implies exclusive access to the data.
        let invariant = PhantomData;
        Self { raw, invariant }
    }
}

impl<T> Deref for SlimSliceMut<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.shared().deref()
    }
}

impl<T> DerefMut for SlimSliceMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: `ptr` and `len` are either
        // a) derived from a live `Box<[T]>` valid for `'self`
        // b) derived from a live `&'self [T]`
        // and additionally, we have the only pointer to the data.
        // so we satisfy all safety requirements for `from_raw_parts_mut`.
        unsafe { self.raw.deref_mut() }
    }
}

impl<T: Debug> Debug for SlimSliceMut<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.deref(), f)
    }
}

impl<T: Hash> Hash for SlimSliceMut<'_, T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self.deref(), state)
    }
}

impl<T: Eq> Eq for SlimSliceMut<'_, T> {}
impl<T: PartialEq> PartialEq for SlimSliceMut<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}
impl<T: PartialEq> PartialEq<[T]> for SlimSliceMut<'_, T> {
    fn eq(&self, other: &[T]) -> bool {
        self.deref() == other
    }
}

impl<T: Ord> Ord for SlimSliceMut<'_, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other)
    }
}
impl<T: PartialOrd> PartialOrd for SlimSliceMut<'_, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}
impl<T: PartialOrd> PartialOrd<[T]> for SlimSliceMut<'_, T> {
    fn partial_cmp(&self, other: &[T]) -> Option<Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<T: Clone> From<&SlimSliceMut<'_, T>> for SlimSliceBox<T> {
    fn from(slice: &SlimSliceMut<'_, T>) -> Self {
        // SAFETY: `slice` is limited to `len: u32` by construction.
        unsafe { Self::from_boxed_unchecked(into_box(slice.deref())) }
    }
}
impl<T: Clone> From<&SlimSliceMut<'_, T>> for Box<[T]> {
    fn from(slice: &SlimSliceMut<'_, T>) -> Self {
        slice.deref().into()
    }
}
impl<T: Clone> From<&SlimSliceMut<'_, T>> for Vec<T> {
    fn from(slice: &SlimSliceMut<'_, T>) -> Self {
        slice.deref().into()
    }
}

impl<T: Clone> From<SlimSliceMut<'_, T>> for SlimSliceBox<T> {
    fn from(slice: SlimSliceMut<'_, T>) -> Self {
        (&slice).into()
    }
}
impl<T: Clone> From<SlimSliceMut<'_, T>> for Box<[T]> {
    fn from(slice: SlimSliceMut<'_, T>) -> Self {
        slice.deref().into()
    }
}
impl<T: Clone> From<SlimSliceMut<'_, T>> for Vec<T> {
    fn from(slice: SlimSliceMut<'_, T>) -> Self {
        slice.deref().into()
    }
}

impl<'a, T> TryFrom<&'a mut [T]> for SlimSliceMut<'a, T> {
    type Error = LenTooLong<&'a mut [T]>;

    fn try_from(slice: &'a mut [T]) -> Result<Self, Self::Error> {
        ensure_len_fits!(slice);
        // SAFETY: ^-- satisfies `len <= u32::MAX`.
        Ok(unsafe { Self::from_slice_unchecked(slice) })
    }
}

/// Converts `&mut [T]` into the slim limited version.
///
/// Panics when `slice.len() > u32::MAX`.
#[inline]
pub fn slice_mut<T>(s: &mut [T]) -> SlimSliceMut<'_, T> {
    expect_fit(s)
}

// =============================================================================
// Shared string slice reference
// =============================================================================

/// A shared reference to `str` limited to `u32::MAX` in length.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct SlimStr<'a> {
    /// The raw byte slice.
    raw: SlimSlice<'a, u8>,
}

impl Deref for SlimStr<'_> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: Data is derived from `str` originally so it's valid UTF-8.
        unsafe { from_utf8_unchecked(self.raw.deref()) }
    }
}

impl Debug for SlimStr<'_> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.deref(), f)
    }
}

impl Display for SlimStr<'_> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.deref(), f)
    }
}

impl Hash for SlimStr<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self.deref(), state)
    }
}

impl Eq for SlimStr<'_> {}
impl PartialEq for SlimStr<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}
impl PartialEq<str> for SlimStr<'_> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.deref() == other
    }
}

impl Ord for SlimStr<'_> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other)
    }
}
impl PartialOrd for SlimStr<'_> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}
impl PartialOrd<str> for SlimStr<'_> {
    #[inline]
    fn partial_cmp(&self, other: &str) -> Option<Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl From<&SlimStr<'_>> for SlimStrBox {
    #[inline]
    fn from(slice: &SlimStr<'_>) -> Self {
        (*slice).into()
    }
}
impl From<&SlimStr<'_>> for Box<str> {
    #[inline]
    fn from(slice: &SlimStr<'_>) -> Self {
        slice.deref().into()
    }
}
impl From<&SlimStr<'_>> for String {
    #[inline]
    fn from(slice: &SlimStr<'_>) -> Self {
        slice.deref().into()
    }
}

impl From<SlimStr<'_>> for SlimStrBox {
    #[inline]
    fn from(slice: SlimStr<'_>) -> Self {
        // SAFETY: `slice` is limited to `len: u32` by construction + UTF-8.
        Self { raw: slice.raw.into() }
    }
}
impl From<SlimStr<'_>> for Box<str> {
    #[inline]
    fn from(slice: SlimStr<'_>) -> Self {
        slice.deref().into()
    }
}
impl From<SlimStr<'_>> for String {
    #[inline]
    fn from(slice: SlimStr<'_>) -> Self {
        slice.deref().into()
    }
}

impl<'a> TryFrom<&'a str> for SlimStr<'a> {
    type Error = LenTooLong<&'a str>;

    #[inline]
    fn try_from(slice: &'a str) -> Result<Self, Self::Error> {
        ensure_len_fits!(slice);
        // SAFETY: ^-- satisfies `len <= u32::MAX`.
        let raw = unsafe { SlimSlice::from_slice_unchecked(slice.as_bytes()) };
        // SAFETY: `slice` is UTF-8.
        Ok(Self { raw })
    }
}

/// Converts `&str` into the slim limited version.
///
/// Panics when `str.len() > u32::MAX`.
#[inline]
pub const fn str(s: &str) -> SlimStr<'_> {
    if s.len() > u32::MAX as usize {
        panic!("length didn't fit in `u32`");
    }

    // SAFETY: ^-- satisfies `len <= u32::MAX`.
    let raw = unsafe { SlimSlice::from_slice_unchecked(s.as_bytes()) };

    // SAFETY: `slice` is UTF-8.
    SlimStr { raw }
}

/// Converts `&str` into the owned slim limited version.
///
/// Panics when `str.len() > u32::MAX`.
#[inline]
pub fn string(s: &str) -> SlimStrBox {
    str(s).into()
}

// =============================================================================
// Mutable string slice reference
// =============================================================================

/// A mutable reference to `str` limited to `u32::MAX` in length.
#[repr(transparent)]
pub struct SlimStrMut<'a> {
    /// The raw byte slice.
    raw: SlimSliceMut<'a, u8>,
}

impl Deref for SlimStrMut<'_> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: Data is derived from `str` originally so it's valid UTF-8.
        unsafe { from_utf8_unchecked(self.raw.deref()) }
    }
}

impl DerefMut for SlimStrMut<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Data is derived from `str` originally so it's valid UTF-8.
        unsafe { from_utf8_unchecked_mut(self.raw.deref_mut()) }
    }
}

impl Debug for SlimStrMut<'_> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.deref(), f)
    }
}

impl Display for SlimStrMut<'_> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.deref(), f)
    }
}

impl Hash for SlimStrMut<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self.deref(), state)
    }
}

impl Eq for SlimStrMut<'_> {}
impl PartialEq for SlimStrMut<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}
impl PartialEq<str> for SlimStrMut<'_> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.deref() == other
    }
}

impl Ord for SlimStrMut<'_> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other)
    }
}
impl PartialOrd for SlimStrMut<'_> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}
impl PartialOrd<str> for SlimStrMut<'_> {
    #[inline]
    fn partial_cmp(&self, other: &str) -> Option<Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl From<&SlimStrMut<'_>> for SlimStrBox {
    #[inline]
    fn from(slice: &SlimStrMut<'_>) -> Self {
        // SAFETY: `slice` is limited to `len: u32` by construction + UTF-8.
        Self {
            raw: (&slice.raw).into(),
        }
    }
}
impl From<&SlimStrMut<'_>> for Box<str> {
    #[inline]
    fn from(slice: &SlimStrMut<'_>) -> Self {
        slice.deref().into()
    }
}
impl From<&SlimStrMut<'_>> for String {
    #[inline]
    fn from(slice: &SlimStrMut<'_>) -> Self {
        slice.deref().into()
    }
}

impl From<SlimStrMut<'_>> for SlimStrBox {
    #[inline]
    fn from(slice: SlimStrMut<'_>) -> Self {
        (&slice).into()
    }
}
impl From<SlimStrMut<'_>> for Box<str> {
    #[inline]
    fn from(slice: SlimStrMut<'_>) -> Self {
        slice.deref().into()
    }
}
impl From<SlimStrMut<'_>> for String {
    #[inline]
    fn from(slice: SlimStrMut<'_>) -> Self {
        slice.deref().into()
    }
}

impl<'a> TryFrom<&'a mut str> for SlimStrMut<'a> {
    type Error = LenTooLong<&'a mut str>;

    #[inline]
    fn try_from(slice: &'a mut str) -> Result<Self, Self::Error> {
        ensure_len_fits!(slice);
        // SAFETY: ^-- satisfies `len <= u32::MAX`.
        let raw = unsafe { SlimSliceMut::from_slice_unchecked(slice.as_bytes_mut()) };
        // SAFETY: `slice` is UTF-8 + mut reference.
        Ok(Self { raw })
    }
}

/// Converts `&mut str` into the slim limited version.
///
/// Panics when `str.len() > u32::MAX`.
#[inline]
pub fn str_mut(s: &mut str) -> SlimStrMut<'_> {
    expect_fit(s)
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;

    use super::*;

    fn hash_of<T: Hash>(x: T) -> u64 {
        let mut hasher = DefaultHasher::new();
        x.hash(&mut hasher);
        hasher.finish()
    }

    fn hash_properties<T>(a: &T, b: &T, a_deref: &T::Target, b_deref: &T::Target)
    where
        T: Hash + Debug + Deref,
        <T as Deref>::Target: Hash,
    {
        assert_eq!(hash_of(a), hash_of(a_deref));
        assert_eq!(hash_of(b), hash_of(b_deref));
        assert_ne!(hash_of(a), hash_of(b));
    }

    fn ord_properties<T: Ord>(a: &T, b: &T, a_deref: &T::Target, b_deref: &T::Target)
    where
        T: Ord + Debug + Deref + PartialOrd<<T as Deref>::Target>,
    {
        assert_eq!(a.partial_cmp(b), Some(Ordering::Less));
        assert_eq!(b.partial_cmp(a), Some(Ordering::Greater));
        assert_eq!(a.partial_cmp(a), Some(Ordering::Equal));
        assert_eq!(b.partial_cmp(b), Some(Ordering::Equal));
        assert_eq!(a.partial_cmp(b_deref), Some(Ordering::Less));
        assert_eq!(b.partial_cmp(a_deref), Some(Ordering::Greater));
        assert_eq!(a.partial_cmp(a_deref), Some(Ordering::Equal));
        assert_eq!(b.partial_cmp(b_deref), Some(Ordering::Equal));

        assert_eq!(a.cmp(b), Ordering::Less);
        assert_eq!(b.cmp(a), Ordering::Greater);
        assert_eq!(a.cmp(a), Ordering::Equal);
        assert_eq!(b.cmp(b), Ordering::Equal);
    }

    #[allow(clippy::eq_op)]
    fn eq_properties<T>(a: &T, b: &T, a_deref: &T::Target, b_deref: &T::Target)
    where
        T: Eq + Debug + Deref + PartialEq<<T as Deref>::Target>,
    {
        assert!(a != b);
        assert!(b != a);
        assert_eq!(a, a);
        assert!(a != b_deref);
        assert!(a == a_deref);
        assert!(b != a_deref);
        assert!(b == b_deref);
    }

    fn debug_properties<T: Debug, U: ?Sized + Debug>(a: &T, b: &T, a_cmp: &U, b_cmp: &U) {
        assert_eq!(format!("{:?}", a), format!("{:?}", a_cmp));
        assert_eq!(format!("{:?}", b), format!("{:?}", b_cmp));
    }

    fn display_properties<T: Debug + Display, U: ?Sized + Display>(a: &T, b: &T, a_cmp: &U, b_cmp: &U) {
        assert_eq!(a.to_string(), a_cmp.to_string());
        assert_eq!(b.to_string(), b_cmp.to_string());
    }

    fn general_properties<T, U>(a: &T, b: &T, a_deref: &U, b_deref: &U)
    where
        T: Deref<Target = U> + Debug + Eq + PartialEq<U> + PartialOrd<U> + Ord + Hash,
        U: ?Sized + Debug + Eq + Ord + Hash,
    {
        eq_properties(a, b, a_deref, b_deref);
        ord_properties(a, b, a_deref, b_deref);
        hash_properties(a, b, a_deref, b_deref);
        debug_properties(a, b, a_deref, b_deref);
    }

    const TEST_STR: &str = "foo";
    const TEST_STR2: &str = "fop";
    const TEST_SLICE: &[u8] = TEST_STR.as_bytes();
    const TEST_SLICE2: &[u8] = TEST_STR2.as_bytes();

    fn test_strings() -> [String; 2] {
        [TEST_STR.to_string(), TEST_STR2.to_string()]
    }

    fn test_slices() -> [Vec<u8>; 2] {
        [TEST_SLICE.to_owned(), TEST_SLICE2.to_owned()]
    }

    fn various_boxed_slices() -> [[SlimSliceBox<u8>; 2]; 5] {
        [
            test_slices().map(SlimSliceBox::from_vec),
            test_slices().map(Box::from).map(SlimSliceBox::from_boxed),
            test_slices().map(|s| SlimSliceBox::try_from(s).unwrap()),
            test_slices().map(|s| SlimSliceBox::try_from(s.into_boxed_slice()).unwrap()),
            test_slices().map(|s| SlimSliceBox::from(<[u8; 3]>::try_from(s).unwrap())),
        ]
    }

    fn various_boxed_strs() -> [[SlimStrBox; 2]; 6] {
        [
            test_strings().map(|s| string(&s)),
            test_strings().map(SlimStrBox::from_string),
            test_strings().map(Box::from).map(SlimStrBox::from_boxed),
            test_strings().map(|s| SlimStrBox::try_from(s).unwrap()),
            test_strings().map(|s| SlimStrBox::try_from(s.into_boxed_str()).unwrap()),
            test_strings().map(|s| SlimStrBox::try_from(s.deref()).unwrap()),
        ]
    }

    fn assert_str_mut_properties(s1: &mut SlimStrMut<'_>, s2: &mut SlimStrMut<'_>) {
        let a: &SlimStrMut<'_> = s1;
        let b: &SlimStrMut<'_> = s2;

        assert_eq!(a.deref(), TEST_STR);
        assert_eq!(SlimStrBox::from(a).clone().deref(), TEST_STR);
        assert_eq!(b.deref(), TEST_STR2);

        assert_eq!(String::from(a), TEST_STR);
        assert_eq!(<Box<str>>::from(a).deref(), TEST_STR);
        assert_eq!(SlimStrBox::from(a).deref(), TEST_STR);
        assert_eq!(<Box<str>>::from(SlimStrBox::from(a)).deref(), TEST_STR);

        general_properties(a, b, TEST_STR, TEST_STR2);
        display_properties(a, b, TEST_STR, TEST_STR2);

        s1.deref_mut().make_ascii_uppercase();
        assert_eq!(&**s1, TEST_STR.to_uppercase());
    }

    #[test]
    fn str_mut_call() {
        let [mut s1, mut s2] = test_strings();
        let s1 = &mut str_mut(s1.as_mut_str());
        let s2 = &mut str_mut(s2.as_mut_str());
        assert_str_mut_properties(s1, s2);
    }

    #[test]
    fn str_mut_try_into() {
        let [mut s1, mut s2] = test_strings();
        let s1: &mut SlimStrMut = &mut s1.as_mut().try_into().unwrap();
        let s2: &mut SlimStrMut = &mut s2.as_mut().try_into().unwrap();
        assert_str_mut_properties(s1, s2);
    }

    #[test]
    fn str_mut_exclusive_ref_various() {
        for [mut a, mut b] in various_boxed_strs() {
            assert_str_mut_properties(a.exclusive_ref(), b.exclusive_ref())
        }
    }

    fn assert_str_properties(a: &SlimStr<'_>, b: &SlimStr<'_>) {
        assert_eq!(a.deref(), TEST_STR);
        assert_eq!(SlimStrBox::from(a).clone().deref(), TEST_STR);
        assert_eq!(b.deref(), TEST_STR2);

        assert_eq!(String::from(a), TEST_STR);
        assert_eq!(<Box<str>>::from(a).deref(), TEST_STR);
        assert_eq!(SlimStrBox::from(a).deref(), TEST_STR);
        assert_eq!(String::from(SlimStrBox::from(a)).deref(), TEST_STR);
        assert_eq!(<Box<str>>::from(SlimStrBox::from(a)).deref(), TEST_STR);

        general_properties(a, b, TEST_STR, TEST_STR2);
        display_properties(a, b, TEST_STR, TEST_STR2);
    }

    #[test]
    fn str_call() {
        let [s1, s2] = test_strings();
        assert_str_properties(&str(&s1), &str(&s2));
    }

    #[test]
    fn str_try_into() {
        let [s1, s2] = test_strings();
        let s1: &SlimStr = &mut s1.deref().try_into().unwrap();
        let s2: &SlimStr = &mut s2.deref().try_into().unwrap();
        assert_str_properties(s1, s2);
    }

    #[test]
    fn str_shared_ref_various() {
        for [a, b] in various_boxed_strs() {
            assert_str_properties(a.shared_ref(), b.shared_ref())
        }
    }

    fn assert_slice_mut_properties(s1: &mut SlimSliceMut<'_, u8>, s2: &mut SlimSliceMut<'_, u8>) {
        let a: &SlimSliceMut<'_, u8> = s1;
        let b: &SlimSliceMut<'_, u8> = s2;

        assert_eq!(a.deref(), TEST_SLICE);
        assert_eq!(SlimSliceBox::from(a).clone().deref(), TEST_SLICE);
        assert_eq!(b.deref(), TEST_SLICE2);

        assert_eq!(<Vec<u8>>::from(a), TEST_SLICE);
        assert_eq!(<Box<[u8]>>::from(a).deref(), TEST_SLICE);
        assert_eq!(<SlimSliceBox<u8>>::from(a).deref(), TEST_SLICE);
        assert_eq!(<Vec<u8>>::from(<SlimSliceBox<u8>>::from(a)).deref(), TEST_SLICE);

        general_properties(a, b, TEST_SLICE, TEST_SLICE2);

        s1.deref_mut().make_ascii_uppercase();
        let mut upper = TEST_SLICE.to_owned();
        upper.iter_mut().for_each(|x| x.make_ascii_uppercase());
        assert_eq!(&**s1, upper);
    }

    #[test]
    fn slice_mut_call() {
        let [mut s1, mut s2] = test_slices();
        let s1 = &mut slice_mut(s1.as_mut());
        let s2 = &mut slice_mut(s2.as_mut());
        assert_slice_mut_properties(s1, s2);
    }

    #[test]
    fn slice_mut_try_into() {
        let [mut s1, mut s2] = test_slices();
        let s1: &mut SlimSliceMut<u8> = &mut s1.deref_mut().try_into().unwrap();
        let s2: &mut SlimSliceMut<u8> = &mut s2.deref_mut().try_into().unwrap();
        assert_slice_mut_properties(s1, s2);
    }

    #[test]
    fn slice_mut_exclusive_ref_various() {
        for [mut a, mut b] in various_boxed_slices() {
            assert_slice_mut_properties(a.exclusive_ref(), b.exclusive_ref());
        }
    }

    fn assert_slice_properties(a: &SlimSlice<'_, u8>, b: &SlimSlice<'_, u8>) {
        assert_eq!(a.deref(), TEST_SLICE);
        assert_eq!(SlimSliceBox::from(a).clone().deref(), TEST_SLICE);
        assert_eq!(b.deref(), TEST_SLICE2);

        assert_eq!(<Vec<u8>>::from(a), TEST_SLICE);
        assert_eq!(<Box<[u8]>>::from(a).deref(), TEST_SLICE);
        assert_eq!(<SlimSliceBox<u8>>::from(a).deref(), TEST_SLICE);
        assert_eq!(<Vec<u8>>::from(<SlimSliceBox<u8>>::from(a)).deref(), TEST_SLICE);

        general_properties(a, b, TEST_SLICE, TEST_SLICE2);
    }

    #[test]
    fn slice_call() {
        let [s1, s2] = test_slices();
        assert_slice_properties(&slice(&s1), &slice(&s2));
    }

    #[test]
    fn slice_try_into() {
        let [s1, s2] = test_slices();
        let s1: &SlimSlice<u8> = &s1.deref().try_into().unwrap();
        let s2: &SlimSlice<u8> = &s2.deref().try_into().unwrap();
        assert_slice_properties(s1, s2);
    }

    #[test]
    fn slice_shared_ref_various() {
        for [a, b] in various_boxed_slices() {
            assert_slice_properties(a.shared_ref(), b.shared_ref())
        }
    }
}
