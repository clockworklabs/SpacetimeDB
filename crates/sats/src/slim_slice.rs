use core::{
    cmp::Ordering,
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut},
    ptr::{slice_from_raw_parts_mut, NonNull},
    slice,
    str::{from_utf8_unchecked, from_utf8_unchecked_mut},
};
use std::borrow::Borrow;

// =============================================================================
// Utils
// =============================================================================

/// Ensures that `$thing.len() <= u32::MAX`.
macro_rules! ensure_len_fits {
    ($thing:expr) => {
        let Ok(_) = u32::try_from($thing.len()) else {
            return Err($thing);
        };
    };
}

/// Expect `Ok(x)` and return it,
/// erroring otherwise with the reason being that `len > u32::MAX`.
fn expect_fit<T, E>(r: Result<T, E>, len: impl FnOnce(E) -> usize) -> T {
    r.map_err(len).expect("length didn't fit in `u32`")
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
    /// The length of the slice.
    len: u32,
    /// A valid pointer to the slice data.
    ptr: NonNull<T>,
}

impl<T> SlimRawSlice<T> {
    /// Split `self` into a raw pointer and the slice length.
    fn split(self) -> (*mut T, usize) {
        (self.ptr.as_ptr(), self.len as usize)
    }

    /// Dereferences this raw slice into a shared slice.
    ///
    /// SAFETY: `self.ptr` and `self.len` must satisfy `from_raw_parts`'s requirements.
    #[allow(clippy::needless_lifetimes)]
    unsafe fn deref<'a>(&'a self) -> &'a [T] {
        let (ptr, len) = self.split();
        // SAFETY: caller is responsible for these.
        unsafe { slice::from_raw_parts(ptr, len) }
    }

    /// Dereferences this raw slice into a mutable slice.
    ///
    /// SAFETY: `self.ptr` and `self.len` must satisfy `from_raw_parts_mut`'s requirements.
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
    const fn from_ptr_len(ptr: *mut T, len: usize) -> Self {
        // SAFETY: caller ensured that `!ptr.is_null()`.
        let ptr = unsafe { NonNull::new_unchecked(ptr) };
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

/// Provides a slimmer version of `Box<[T]>`
/// using `u32` for its length instead of `usize`.
#[repr(transparent)]
pub struct SlimSliceBox<T> {
    /// The representation of this boxed slice.
    raw: SlimRawSlice<T>,
    /// Marker to ensure covariance and dropck ownership.
    owned: PhantomData<T>,
}

/// `SlimSlice<T>` is `Send` if `T` is `Send` because the data is owned.
unsafe impl<T: Send> Send for SlimSliceBox<T> {}

/// `SlimSlice<T>` is `Sync` if `T` is `Sync` because the data is owned.
unsafe impl<T: Sync> Sync for SlimSliceBox<T> {}

impl<T> Drop for SlimSliceBox<T> {
    fn drop(&mut self) {
        // Get us an owned `SliceSlice<T>`
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
    unsafe fn from_boxed_unchecked(mut boxed: Box<[T]>) -> Self {
        // SAFETY: `Box<T>`'s ptr was a `NonNull<T>` already.
        let raw = SlimRawSlice::from_ptr_len(boxed.as_mut_ptr(), boxed.len());
        let owned = PhantomData;
        Self { raw, owned }
    }

    /// Converts `boxed: Box<[T]>` into `SlimSlice<T>`.
    ///
    /// Panics when `boxed.len() > u32::MAX`.
    pub fn from_boxed(boxed: Box<[T]>) -> Self {
        expect_fit(Self::try_from(boxed), |x| x.len())
    }

    /// Converts `vec: Vec<T>` into `SlimSlice<T>`.
    ///
    /// Panics when `vec.len() > u32::MAX`.
    pub fn from_vec(vec: Vec<T>) -> Self {
        Self::from_boxed(vec.into())
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

impl<T> Deref for SlimSliceBox<T> {
    type Target = [T];

    #[allow(clippy::needless_lifetimes)]
    fn deref<'a>(&'a self) -> &'a Self::Target {
        // SAFETY: `ptr` and `len` are derived from a live `Box<[T]>` valid for `'self`
        // so we satisfy all safety requirements for `from_raw_parts`.
        unsafe { self.raw.deref() }
    }
}

impl<T> DerefMut for SlimSliceBox<T> {
    #[allow(clippy::needless_lifetimes)]
    fn deref_mut<'a>(&'a mut self) -> &'a mut Self::Target {
        // SAFETY: `ptr` and `len` are derived from a live `Box<[T]>`
        // so we satisfy all safety requirements for `from_raw_parts`.
        unsafe { self.raw.deref_mut() }
    }
}

impl<T> TryFrom<Box<[T]>> for SlimSliceBox<T> {
    type Error = Box<[T]>;

    fn try_from(boxed: Box<[T]>) -> Result<Self, Self::Error> {
        ensure_len_fits!(boxed);
        // SAFETY: Checked above that `len <= u32::MAX`.
        Ok(unsafe { Self::from_boxed_unchecked(boxed) })
    }
}

impl<T> TryFrom<Vec<T>> for SlimSliceBox<T> {
    type Error = Vec<T>;

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

impl<T> From<SlimSliceBox<T>> for Box<[T]> {
    fn from(slice: SlimSliceBox<T>) -> Self {
        let (ptr, len) = slice.raw.split();
        // SAFETY: `ptr` and `len` originally came from a `Box<[T]>`.
        unsafe { Box::from_raw(slice_from_raw_parts_mut(ptr, len)) }
    }
}

impl<T> From<SlimSliceBox<T>> for Vec<T> {
    fn from(slice: SlimSliceBox<T>) -> Self {
        into_box(slice).into()
    }
}

impl<T: Debug> Debug for SlimSliceBox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self, f)
    }
}

impl<T: Clone> Clone for SlimSliceBox<T> {
    fn clone(&self) -> Self {
        // Allocate exactly the right amount
        // so we later don't reallocate due to excess capacity.
        let mut vec = Vec::with_capacity(self.len());
        vec.clone_from_slice(self);
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
    pub inner: Result<SlimSliceBox<T>, Vec<T>>,
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
    /// Converts `boxed: Box<str>` into `SlimStr`.
    ///
    /// Panics when `boxed.len() > u32::MAX`.
    #[inline]
    pub fn from_boxed(boxed: Box<str>) -> Self {
        expect_fit(Self::try_from(boxed), |x| x.len())
    }

    /// Converts `str: String` into `SlimStr`.
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
        // SAFETY: By construction all `SlimStr`s are valid UTF-8.
        unsafe { from_utf8_unchecked(&self.raw) }
    }
}

impl DerefMut for SlimStrBox {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: By construction all `SlimStr`s are valid UTF-8.
        unsafe { from_utf8_unchecked_mut(&mut self.raw) }
    }
}

impl TryFrom<Box<str>> for SlimStrBox {
    type Error = Box<str>;

    #[inline]
    fn try_from(boxed: Box<str>) -> Result<Self, Self::Error> {
        ensure_len_fits!(boxed);
        // SAFETY: Checked above that `len <= u32::MAX`.
        let raw = unsafe { SlimSliceBox::from_boxed_unchecked(into_box(boxed)) };
        Ok(Self { raw })
    }
}

impl TryFrom<String> for SlimStrBox {
    type Error = String;

    #[inline]
    fn try_from(str: String) -> Result<Self, Self::Error> {
        ensure_len_fits!(str);
        // SAFETY: Checked above that `len <= u32::MAX`.
        let raw = unsafe { SlimSliceBox::from_boxed_unchecked(into_box(str.into_boxed_str())) };
        Ok(Self { raw })
    }
}

impl<'a> TryFrom<&'a str> for SlimStrBox {
    type Error = &'a str;

    #[inline]
    fn try_from(str: &'a str) -> Result<Self, Self::Error> {
        str.try_into().map(|s: SlimStr<'_>| s.into())
    }
}

impl From<SlimStrBox> for Box<str> {
    fn from(str: SlimStrBox) -> Self {
        let raw_box = into_box(str.raw);
        // SAFETY: By construction, `SlimStr` is valid UTF-8.
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
        Debug::fmt(&self, f)
    }
}

impl Display for SlimStrBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self, f)
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
        **self == **other
    }
}

impl Eq for SlimStrBox {}

impl<R: Deref> PartialOrd<R> for SlimStrBox
where
    str: PartialOrd<R::Target>,
{
    fn partial_cmp(&self, other: &R) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl Ord for SlimStrBox {
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw.cmp(&other.raw)
    }
}

impl Hash for SlimStrBox {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&**self, state)
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

/// A shared reference to `[T]` limited to `u32::MAX` in length.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct SlimSlice<'a, T> {
    /// The representation of this shared slice.
    raw: SlimRawSlice<T>,
    /// Marker to ensure covariance for `'a`.
    covariant: PhantomData<&'a [T]>,
}

// SAFETY: Same rules as for `&[T]`.
unsafe impl<T: Send + Sync> Send for SlimSlice<'_, T> {}

// SAFETY: Same rules as for `&[T]`.
unsafe impl<T: Sync> Sync for SlimSlice<'_, T> {}

impl<'a, T> SlimSlice<'a, T> {
    /// Converts a `&[T]` to the limited version without length checking.
    ///
    /// SAFETY: `slice.len() <= u32::MAX` must hold.
    const unsafe fn from_slice_unchecked(slice: &'a [T]) -> Self {
        // SAFETY: `&mut [T]` implies that the pointer is non-null.
        let raw = SlimRawSlice::from_ptr_len(slice.as_ptr().cast_mut(), slice.len());
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

impl<T: Clone> From<SlimSlice<'_, T>> for SlimSliceBox<T> {
    fn from(slice: SlimSlice<'_, T>) -> Self {
        // SAFETY: `slice` is limited to `len: u32` by construction.
        unsafe { Self::from_boxed_unchecked(into_box(slice)) }
    }
}
impl<T: Clone> From<SlimSlice<'_, T>> for Box<[T]> {
    fn from(slice: SlimSlice<'_, T>) -> Self {
        slice.into()
    }
}
impl<T: Clone> From<SlimSlice<'_, T>> for Vec<T> {
    fn from(slice: SlimSlice<'_, T>) -> Self {
        slice.into()
    }
}

impl<'a, T> TryFrom<&'a [T]> for SlimSlice<'a, T> {
    type Error = &'a [T];

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
    expect_fit(s.try_into(), |x| x.len())
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
    pub fn shared(&'a mut self) -> &'a SlimSlice<'a, T> {
        // SAFETY: By virtue of `&'a mut X -> &'a X` being safe, this is also.
        // The types and references to them have the same layout as well.
        unsafe { mem::transmute(self) }
    }

    /// Converts a `&mut [T]` to the limited version without length checking.
    ///
    /// SAFETY: `slice.len() <= u32::MAX` must hold.
    unsafe fn from_slice_unchecked(slice: &'a mut [T]) -> Self {
        // SAFETY: `&mut [T]` implies that the pointer is non-null.
        let raw = SlimRawSlice::from_ptr_len(slice.as_mut_ptr(), slice.len());
        // SAFETY: Our invariants are satisfied by the caller
        // and that `&mut [T]` implies exclusive access to the data.
        let invariant = PhantomData;
        Self { raw, invariant }
    }
}

impl<T> Deref for SlimSliceMut<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        // SAFETY: `ptr` and `len` are either
        // a) derived from a live `Box<[T]>` valid for `'self`
        // b) derived from a live `&'self mut [T]`
        // so we satisfy all safety requirements for `from_raw_parts`.
        unsafe { self.raw.deref() }
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

impl<T: Clone> From<SlimSliceMut<'_, T>> for SlimSliceBox<T> {
    fn from(slice: SlimSliceMut<'_, T>) -> Self {
        // SAFETY: `slice` is limited to `len: u32` by construction.
        unsafe { Self::from_boxed_unchecked(into_box(slice)) }
    }
}
impl<T: Clone> From<SlimSliceMut<'_, T>> for Box<[T]> {
    fn from(slice: SlimSliceMut<'_, T>) -> Self {
        slice.into()
    }
}
impl<T: Clone> From<SlimSliceMut<'_, T>> for Vec<T> {
    fn from(slice: SlimSliceMut<'_, T>) -> Self {
        slice.into()
    }
}

impl<'a, T> TryFrom<&'a mut [T]> for SlimSliceMut<'a, T> {
    type Error = &'a mut [T];

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
    expect_fit(s.try_into(), |x| x.len())
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
        unsafe { from_utf8_unchecked(&self.raw) }
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
        slice.into()
    }
}
impl From<SlimStr<'_>> for String {
    #[inline]
    fn from(slice: SlimStr<'_>) -> Self {
        slice.into()
    }
}

impl<'a> TryFrom<&'a str> for SlimStr<'a> {
    type Error = &'a str;

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
        unsafe { from_utf8_unchecked(&self.raw) }
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

impl From<SlimStrMut<'_>> for SlimStrBox {
    #[inline]
    fn from(slice: SlimStrMut<'_>) -> Self {
        // SAFETY: `slice` is limited to `len: u32` by construction + UTF-8.
        Self { raw: slice.raw.into() }
    }
}
impl From<SlimStrMut<'_>> for Box<str> {
    #[inline]
    fn from(slice: SlimStrMut<'_>) -> Self {
        slice.into()
    }
}
impl From<SlimStrMut<'_>> for String {
    #[inline]
    fn from(slice: SlimStrMut<'_>) -> Self {
        slice.into()
    }
}

impl<'a> TryFrom<&'a mut str> for SlimStrMut<'a> {
    type Error = &'a mut str;

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
    expect_fit(s.try_into(), |x| x.len())
}
