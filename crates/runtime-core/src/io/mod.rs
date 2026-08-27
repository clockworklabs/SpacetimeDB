mod buf;
pub use buf::AlignedBytes;
#[cfg(feature = "alloc")]
pub use buf::{ErasedBox, ErasedBoxPtr};

mod error;
pub use error::ErrorWith;

/// Size in bytes of a disk sector.
pub const SECTOR_SIZE: usize = 4096;

/// Subset of the `statx` metadata.
#[derive(Debug)]
#[non_exhaustive]
pub struct Statx {
    pub size: u64,
}

impl Statx {
    pub fn from_size(size: u64) -> Self {
        Self { size }
    }
}

/// The canonical, low-level I/O API.
///
/// Currently only supports file I/O, but eventually all I/O performed by
/// SpacetimeDB should go through this trait.
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
    /// This should always be instantiated to [std::io::Error]. However, pending
    /// [alloc_io], this type is not in `core`, which would prevent this crate
    /// from being `no_std`.
    ///
    /// [alloc_io]: https://github.com/rust-lang/rust/issues/154046
    type Error: core::error::Error;
    /// The completion [Future] of all methods in this trait.
    type Completion<T>: Future<Output = T> + Unpin;

    /// Open the file at `path`.
    fn open_file(&self, path: &str) -> Self::Completion<Result<Self::Fd, Self::Error>>;

    /// Create the file at `path`.
    ///
    /// Returns an error if the file already exists.
    fn create_file(&self, path: &str) -> Self::Completion<Result<Self::Fd, Self::Error>>;

    /// Write `buf` to `fd` at `offset`.
    ///
    /// `offset` must be a multiple of [SECTOR_SIZE].
    ///
    /// Behaves like `FileExt::write_all_at`, i.e. tries to write all bytes in
    /// `buf`, potentially retrying on errors of kind interrupted, and returns
    /// an error if that fails.
    fn write_all_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>>;

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
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>>;

    /// Call `fsync(2)` on `fd`.
    fn fsync(&self, fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>>;
    /// Call `fdatasync(2)` on `fd`.
    fn fdatasync(&self, fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>>;

    /// Allocate `total` bytes for the file `fd`.
    ///
    /// Implementations must ensure that attempts to shrink the file result in
    /// an error. The operation should succeed if the file's size is already
    /// `total`.
    fn reserve(&self, fd: Self::Fd, total: u64) -> Self::Completion<Result<(), Self::Error>>;

    /// Determine the length of the file `fd`.
    ///
    /// This should not depend on `fsync`, i.e. `statx`. See `std::io::Seek::stream_len`.
    fn statx(&self, fd: Self::Fd) -> Self::Completion<Result<Statx, Self::Error>>;
}
