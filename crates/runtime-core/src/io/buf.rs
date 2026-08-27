use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::io::SECTOR_SIZE;

/// Types that can be safely converted to and from sector-aligned byte slices.
pub trait AlignedBytes: Sized {
    /// Assert that the type' size is a multiple of [SECTOR_SIZE] and has the
    /// right alignment.
    ///
    /// The type must also not rely on drop glue, i.e. `!core::mem::needs_drop()`.
    ///
    /// NOTE: Associated constants are evaluated lazily -- add a free
    ///
    /// `const _: () = <T as AlignedBytes>::ASSERT_VALID_LAYOUT;`
    ///
    /// for each `T` that is supposed to be used as an `AlignedBytes`.
    const ASSERT_VALID_LAYOUT: () = {
        assert!(align_of::<Self>() == SECTOR_SIZE);
        assert!(size_of::<Self>().is_multiple_of(SECTOR_SIZE));
        assert!(!core::mem::needs_drop::<Self>());
    };

    /// Reinterpret `self` as a byte slice.
    ///
    /// The returned slice will be of length `size_of::<Self>()`.
    fn as_bytes(&self) -> &[u8];

    /// Reinterpret `self` as a mutable byte slice.
    ///
    /// The returned slice will be of length `size_of::<Self>()`.
    fn as_bytes_mut(&mut self) -> &mut [u8];

    /// Reinterpret a byte slice as `Self`.
    ///
    /// The slice must be of length `size_of::<Self>()`.
    ///
    /// NOTE: Any slice of the right size, but consisting of only `0` (zero)
    /// bytes can be converted to `Self`. It is the caller's responsibility to
    /// validate the returned type as per the application's invariants.
    ///
    /// # Panics
    ///
    /// Panics if `b.len() != size_of::<Self>()`.
    fn from_bytes(b: &[u8]) -> Self;
}

impl<T: FromBytes + IntoBytes + KnownLayout + Immutable> AlignedBytes for T {
    fn as_bytes(&self) -> &[u8] {
        <T as IntoBytes>::as_bytes(self)
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        <T as IntoBytes>::as_mut_bytes(self)
    }

    fn from_bytes(b: &[u8]) -> Self {
        Self::read_from_bytes(b).unwrap()
    }
}

#[cfg(feature = "alloc")]
mod boxed {
    use alloc::boxed::Box;
    use core::{alloc::Layout, any::TypeId, ptr::NonNull};

    use crate::io::AlignedBytes;

    /// A type-erased [AlignedBytes] heap allocation.
    pub struct ErasedBox {
        ptr: NonNull<u8>,
        len: usize,
        layout: Layout,
        ty: TypeId,
    }

    impl ErasedBox {
        /// Create an [ErasedBox] from `B` by allocating a new [Box].
        pub fn from_aligned<B: AlignedBytes + 'static>(b: B) -> Self {
            Self::from_aligned_box(Box::new(b))
        }

        /// Create an [ErasedBox] from an already-boxed `B`.
        pub fn from_aligned_box<B: AlignedBytes + 'static>(b: Box<B>) -> Self {
            let () = B::ASSERT_VALID_LAYOUT;

            let ptr = Box::into_raw(b);
            Self {
                ptr: NonNull::new(ptr.cast()).unwrap(),
                len: size_of::<B>(),
                layout: Layout::from_size_align(size_of::<B>(), align_of::<B>()).unwrap(),
                ty: TypeId::of::<B>(),
            }
        }

        /// Reify `B` via casting.
        pub fn into_aligned<B: AlignedBytes + 'static>(self) -> B {
            *Self::into_aligned_box(self)
        }

        /// Reify `B` via casting, without unboxing.
        pub fn into_aligned_box<B: AlignedBytes + 'static>(self) -> Box<B> {
            assert_eq!(self.len, size_of::<B>());
            assert_eq!(self.ty, TypeId::of::<B>());

            let boxed = unsafe { Box::from_raw(self.ptr.as_ptr().cast::<B>()) };
            // Prevent drop, which would deallocate.
            core::mem::forget(self);

            boxed
        }

        pub fn as_ptr(&self) -> ErasedBoxPtr {
            ErasedBoxPtr {
                ptr: self.ptr.as_ptr(),
                len: self.len,
            }
        }
    }

    impl Drop for ErasedBox {
        fn drop(&mut self) {
            unsafe { alloc::alloc::dealloc(self.ptr.as_ptr(), self.layout) }
        }
    }

    pub struct ErasedBoxPtr {
        ptr: *mut u8,
        len: usize,
    }

    impl ErasedBoxPtr {
        pub fn as_bytes(&mut self) -> &[u8] {
            unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
        }

        pub fn as_bytes_mut(&mut self) -> &mut [u8] {
            unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[repr(C, align(4096))]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        struct Trivial([u8; 4096]);

        impl AlignedBytes for Trivial {
            fn as_bytes(&self) -> &[u8] {
                &self.0
            }

            fn as_bytes_mut(&mut self) -> &mut [u8] {
                &mut self.0
            }

            fn from_bytes(b: &[u8]) -> Self {
                assert_eq!(b.len(), size_of::<Self>());
                let mut a = [0; 4096];
                a.copy_from_slice(b);
                Self(a)
            }
        }

        #[test]
        fn roundtrip_preserves_value() {
            let t = Trivial([32; 4096]);

            let erased = ErasedBox::from_aligned(t);
            let reified = erased.into_aligned::<Trivial>();

            assert_eq!(reified, t);
        }
    }
}
#[cfg(feature = "alloc")]
pub use boxed::{ErasedBox, ErasedBoxPtr};
