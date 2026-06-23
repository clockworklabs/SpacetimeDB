#![no_std]

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Size in bytes of a disk sector.
pub const SECTOR_SIZE: usize = 4096;

/// Types that can be safely converted to and from sector-aligned byte slices.
pub trait AlignedBytes: Sized {
    /// Assert that the type' size is a multiple of [SECTOR_SIZE] and has the
    /// right aligment.
    ///
    /// NOTE: Associated constants are evaluated lazily -- add a free
    ///
    /// `const _: () = <T as AlignedBytes>::ASSERT_VALID_LAYOUT;`
    ///
    /// for each `T` that is supposed to be used as an `AlignedBytes`.
    const ASSERT_VALID_LAYOUT: () = {
        assert!(align_of::<Self>() == SECTOR_SIZE);
        assert!(size_of::<Self>().is_multiple_of(SECTOR_SIZE));
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

/// An error `E`, along with auxiliary data `T`.
///
/// `T` is usually a buffer of type [AlignedBytes], whose ownership is
/// transferred back to the caller when an error occurs.
///
/// As this type signifies an error condition, the contents of `T` are
/// unspecified.
pub struct ErrorWith<E, T> {
    pub error: E,
    pub with: T,
}

/// The canonical, low-level I/O API.
///
/// Currently only supports file I/O.
///
/// Intended to support implementations based on `io-uring`, which means that
/// buffer ownership is transferred to the I/O engine while reading or writing.
///
/// Implementations should be `!Send`, i.e. all I/O happens on a single thread.
///
/// File operations should never be mutually exclusive, and therefore expose a
/// `pwrite`/`pread`-style API. It is assumed that direct I/O (`O_DIRECT`) is
/// used, i.e. the kernel page cache is bypassed. The [AlignedBytes] type
/// ensures that the alignment requirements for direct I/O are met.
pub trait SpacetimeIO {
    /// An open file handle.
    ///
    /// Like [std::fs::File], the file shall be closed when the last reference
    /// to the handle is dropped.
    ///
    /// Unlike [std::fs::File], the file handle must be clone-able.
    type Fd: Clone;
    /// The error returned by methods of this trait.
    ///
    /// This should always be instantated to [std::io::Error]. However, pending
    /// https://github.com/rust-lang/rust/issues/154046, this type is not in
    /// `core`, which would prevent this crate to be `no_std`.
    type Error;

    /// Open the file at `path`.
    fn open_file(&self, path: &str) -> impl Future<Output = Result<Self::Fd, Self::Error>>;

    /// Create the file at `path` and allocate `len` bytes.
    ///
    /// Returns an error if the file already exists.
    fn create_file(&self, path: &str, len: u64) -> impl Future<Output = Result<Self::Fd, Self::Error>>;

    /// Write `buf` to `fd` at `offset`.
    ///
    /// `offset` must be a multiple of [SECTOR_SIZE].
    ///
    /// Behaves like `FileExt::write_all_at`, i.e. tries to write all bytes in
    /// `buf`, potentially retrying on errors of kind interrupted, and returns
    /// an error if that fails.
    fn write_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> impl Future<Output = Result<B, ErrorWith<Self::Error, B>>>;

    /// Read `size_of::<B>()` bytes from `fd` at `offset` and interpret them at
    /// type `B`.
    ///
    /// `offset` must be a multiple of [SECTOR_SIZE].
    ///
    /// Behaves like `FileExt::read_exact_at`, i.e. attempts to read
    /// `size_of::<B>()` bytes, potentially retrying on errors of kind
    /// interrupted, and returns an error if less than the required bytes could
    /// be read.
    fn read_exact_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> impl Future<Output = Result<B, ErrorWith<Self::Error, B>>>;

    /// Call `fsync(2)` on `fd`.
    fn fsync(&self, fd: Self::Fd) -> impl Future<Output = Result<(), Self::Error>>;
    /// Call `fdatasync(2)` on `fd`.
    fn fdatasync(&self, fd: Self::Fd) -> impl Future<Output = Result<(), Self::Error>>;

    /// Allocate `additional` bytes for the file `fd`.
    fn reserve(&self, fd: Self::Fd, additional: u64) -> impl Future<Output = Result<(), Self::Error>>;
}
