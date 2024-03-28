use std::{
    alloc,
    ops::{Deref, DerefMut},
    ptr,
};

/// A byte buffer of non-zero size and proper alignment.
#[derive(Debug)]
pub struct Aligned<const SIZE: usize, const ALIGN: usize> {
    data: ptr::NonNull<u8>,
    layout: alloc::Layout,
    size: usize,
}

impl<const SIZE: usize, const ALIGN: usize> Aligned<SIZE, ALIGN> {
    /// Create a new buffer of `SIZE` and `ALIGN`.
    ///
    /// The parameters are implicitly adjusted if they don't satisfy the
    /// invariants of [`alloc::Layout`];
    ///
    /// * `ALIGN` is made a power of two if it isn't already
    /// * `ALIGN` defaults to 512 if it would otherwise overflow
    /// * the maximum `SIZE` is `isize::MAX - (align - 1)` and will
    ///   be adjusted if necessary
    #[allow(clippy::let_unit_value)]
    pub fn new() -> Self {
        _ = <Self as AssertNonZero>::QED;

        let align = ALIGN.checked_next_power_of_two().unwrap_or(512);
        let max_size = isize::MAX as usize - (align - 1);
        let size = if SIZE > max_size { max_size } else { SIZE };

        // SAFETY: Invariants checked above: `align` is non-zero and a power of
        // two, `size` rounded up to the nearest multiple of `align` doesn't
        // overflow isize.
        let layout = unsafe { alloc::Layout::from_size_align_unchecked(size, align) };
        // SAFETY: `Layout` has non-zero size.
        let ptr = unsafe { alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }
        // SAFETY: We just checked that `ptr` is non-null.
        let data = unsafe { ptr::NonNull::new_unchecked(ptr) };

        Self { data, layout, size }
    }

    /// Borrow the buffer as a slice.
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: The allocation has non-zero size `self.size`, is properly
        // aligned, and initialized. `self.size` cannot be modified outside
        // `Self`.
        unsafe { std::slice::from_raw_parts(self.data.as_ptr(), self.size) }
    }

    /// Borrow the buffer as a mutable slice.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: The allocation has non-zero size `self.size`, is properly
        // aligned, and initialized. `self.size` cannot be modified outside
        // `Self`.
        unsafe { std::slice::from_raw_parts_mut(self.data.as_ptr(), self.size) }
    }

    /// The size of the buffer.
    pub const fn len(&self) -> usize {
        self.size
    }

    /// `true` if the buffer is empty.
    ///
    /// This always returns `false`, because the buffer is fully allocated upon
    /// construction and the size cannot be modified.
    pub const fn is_empty(&self) -> bool {
        false
    }
}

impl<const SIZE: usize, const ALIGN: usize> Default for Aligned<SIZE, ALIGN> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: The data referenced from `ptr::NonNull` inside `Aligned` is unaliased.
unsafe impl<const SIZE: usize, const ALIGN: usize> Send for Aligned<SIZE, ALIGN> {}
// SAFETY: The data referenced from `ptr::NonNull` inside `Aligned` is unaliased.
unsafe impl<const SIZE: usize, const ALIGN: usize> Sync for Aligned<SIZE, ALIGN> {}

impl<const SIZE: usize, const ALIGN: usize> Drop for Aligned<SIZE, ALIGN> {
    fn drop(&mut self) {
        // SAFETY: `self.data` was allocated using `self.layout`.
        unsafe { alloc::dealloc(self.data.as_ptr(), self.layout) }
    }
}

impl<const SIZE: usize, const ALIGN: usize> Deref for Aligned<SIZE, ALIGN> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl<const SIZE: usize, const ALIGN: usize> DerefMut for Aligned<SIZE, ALIGN> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_bytes_mut()
    }
}

impl<const SIZE: usize, const ALIGN: usize> AsRef<[u8]> for Aligned<SIZE, ALIGN> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl<const SIZE: usize, const ALIGN: usize> AsMut<[u8]> for Aligned<SIZE, ALIGN> {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

trait AssertNonZero {
    const QED: ();
}

impl<const SIZE: usize, const ALIGN: usize> AssertNonZero for Aligned<SIZE, ALIGN> {
    const QED: () = {
        assert!(SIZE > 0, "size must be nonzero");
        assert!(ALIGN > 0, "align must be nonzero");
    };
}
