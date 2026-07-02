//! Linux file I/O contract backed by an in-tree raw `io_uring` driver.
//!
//! The core rules are fixed here:
//!
//! - operations are explicit-offset, not cursor-based
//! - buffers are owned values leased from a runtime-global pool
//! - submitting an operation moves buffer ownership into the runtime
//! - dropping the completion future detaches the operation instead of blocking
//!   or treating future-drop as cancellation
//!
//! The public API is intentionally higher-level than the kernel ABI. The
//! implementation below owns the `io_uring` setup, mmap layout, SQ/CQ
//! bookkeeping, and opcode encoding directly rather than depending on a Rust
//! wrapper crate.

use std::{
    collections::BTreeMap,
    ffi::{c_void, CString},
    fmt,
    future::Future,
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::ffi::OsStrExt,
    },
    path::{Component, Path, PathBuf},
    pin::Pin,
    ptr,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    task::{Context, Poll},
    thread,
};

use tokio::sync::oneshot;

const DEFAULT_BUFFER_ALIGNMENT: usize = 4096;
const DEFAULT_MAX_CACHED_BUCKET_LEN: usize = 32;
const DEFAULT_MAX_CACHED_BUFFER_CAPACITY: usize = 8 * 1024 * 1024;
const DEFAULT_RING_ENTRIES: u32 = 256;
const IORING_FSYNC_DATASYNC: u32 = 1;
const IORING_FEAT_SINGLE_MMAP: u32 = 1;
const IORING_ENTER_GETEVENTS: u32 = 1;

const IORING_OFF_SQ_RING: u64 = 0;
const IORING_OFF_CQ_RING: u64 = 0x0800_0000;
const IORING_OFF_SQES: u64 = 0x1000_0000;

const IORING_OP_FSYNC: u8 = 3;
const IORING_OP_ASYNC_CANCEL: u8 = 14;
const IORING_OP_FALLOCATE: u8 = 17;
const IORING_OP_OPENAT: u8 = 18;
const IORING_OP_CLOSE: u8 = 19;
const IORING_OP_READ: u8 = 22;
const IORING_OP_WRITE: u8 = 23;
const IORING_OP_WRITEV: u8 = 2;
const IORING_OP_RENAMEAT: u8 = 35;
const IORING_OP_UNLINKAT: u8 = 36;
const IORING_OP_MKDIRAT: u8 = 37;
const IORING_OP_LINKAT: u8 = 39;
const IORING_OP_FTRUNCATE: u8 = 54;

/// Result shape for single-buffer I/O operations.
pub type OwnedBufResult<B> = io::Result<(B, usize)>;

/// Runtime-global entrypoint for Linux file I/O.
///
/// The runtime owns allocation policy and buffer recycling. Callers may hold on
/// to leased buffers for as long as they want, but must move ownership into an
/// in-flight operation instead of borrowing raw slices across submission.
#[derive(Clone)]
pub struct UringRuntime {
    driver: Arc<DriverHandle>,
    pool: Arc<BufferPool>,
}

impl fmt::Debug for UringRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UringRuntime")
            .field("pool", &self.pool.config)
            .finish_non_exhaustive()
    }
}

impl UringRuntime {
    /// Create a runtime with a custom buffer-pool policy.
    pub fn new(config: BufferPoolConfig) -> io::Result<Self> {
        Ok(Self {
            driver: global_driver()?,
            pool: Arc::new(BufferPool::new(config)),
        })
    }

    /// Get the process-global runtime instance.
    pub fn global() -> io::Result<Self> {
        static GLOBAL: OnceLock<io::Result<UringRuntime>> = OnceLock::new();
        match GLOBAL.get_or_init(|| UringRuntime::new(BufferPoolConfig::default())) {
            Ok(runtime) => Ok(runtime.clone()),
            Err(err) => Err(io::Error::new(err.kind(), err.to_string())),
        }
    }

    /// Lease a reusable owned buffer with at least `capacity` bytes.
    pub fn alloc(&self, capacity: usize) -> OwnedBuf {
        OwnedBuf::new(self.pool.clone(), self.pool.acquire(capacity))
    }

    /// Lease an owned buffer and zero-initialize `len` bytes for immediate use.
    pub fn alloc_zeroed(&self, len: usize) -> OwnedBuf {
        let mut buf = self.alloc(len);
        buf.resize_zeroed(len);
        buf
    }

    /// Open a file asynchronously using Linux open flags captured by
    /// [`UringOpenOptions`].
    pub fn open(&self, path: impl Into<PathBuf>, options: UringOpenOptions) -> UringOp<UringFile> {
        let runtime = self.clone();
        let path = path.into();
        self.submit(move |complete| {
            let (flags, mode) = options.to_open_flags()?;
            let path = path_to_cstring(&path)?;
            let sqe = sqe_openat(libc::AT_FDCWD, path.as_ptr(), flags, mode);
            Ok(DriverSubmission::new(
                sqe,
                CompletionResources::Path(path),
                Box::new(move |result, _resources| {
                    let output = result.and_then(|cqe| {
                        let fd = cqe.result;
                        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
                        Ok(UringFile::from_owned_fd(runtime, owned_fd))
                    });
                    let _ = complete.send(output);
                }),
            ))
        })
    }

    /// Create or truncate a writable file.
    pub fn create(&self, path: impl Into<PathBuf>) -> UringOp<UringFile> {
        self.open(
            path,
            UringOpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o644),
        )
    }

    /// Recursively create a directory path using `mkdirat(2)` requests.
    pub async fn create_dir_all(&self, path: impl Into<PathBuf>) -> io::Result<()> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Ok(());
        }

        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component.as_os_str());
            if matches!(component, Component::RootDir | Component::CurDir) {
                continue;
            }

            match self.mkdir(current.clone(), 0o777).await {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }

    /// Atomically rename `from` to `to` within the filesystem's rename rules.
    pub fn rename(&self, from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> UringOp<()> {
        let from = from.into();
        let to = to.into();
        self.submit(move |complete| {
            let from = path_to_cstring(&from)?;
            let to = path_to_cstring(&to)?;
            let sqe = sqe_renameat(libc::AT_FDCWD, from.as_ptr(), libc::AT_FDCWD, to.as_ptr(), 0);
            Ok(unit_submission(sqe, CompletionResources::PathPair(from, to), complete))
        })
    }

    /// Create a hard link.
    pub fn hard_link(&self, src: impl Into<PathBuf>, dst: impl Into<PathBuf>) -> UringOp<()> {
        let src = src.into();
        let dst = dst.into();
        self.submit(move |complete| {
            let src = path_to_cstring(&src)?;
            let dst = path_to_cstring(&dst)?;
            let sqe = sqe_linkat(libc::AT_FDCWD, src.as_ptr(), libc::AT_FDCWD, dst.as_ptr(), 0);
            Ok(unit_submission(sqe, CompletionResources::PathPair(src, dst), complete))
        })
    }

    /// Remove a file path.
    pub fn unlink(&self, path: impl Into<PathBuf>) -> UringOp<()> {
        let path = path.into();
        self.submit(move |complete| {
            let path = path_to_cstring(&path)?;
            let sqe = sqe_unlinkat(libc::AT_FDCWD, path.as_ptr(), 0);
            Ok(unit_submission(sqe, CompletionResources::Path(path), complete))
        })
    }

    /// Sync a directory's metadata to durable storage.
    pub async fn sync_dir(&self, path: impl Into<PathBuf>) -> io::Result<()> {
        let dir = self
            .open(path, UringOpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY))
            .await?;
        dir.fsync().await
    }

    fn mkdir(&self, path: PathBuf, mode: libc::mode_t) -> UringOp<()> {
        self.submit(move |complete| {
            let path = path_to_cstring(&path)?;
            let sqe = sqe_mkdirat(libc::AT_FDCWD, path.as_ptr(), mode);
            Ok(unit_submission(sqe, CompletionResources::Path(path), complete))
        })
    }

    fn submit<T: Send + 'static>(
        &self,
        build: impl FnOnce(oneshot::Sender<io::Result<T>>) -> io::Result<DriverSubmission>,
    ) -> UringOp<T> {
        let (complete, rx) = oneshot::channel();
        let submission = match build(complete) {
            Ok(submission) => submission,
            Err(err) => return UringOp::ready(Err(err)),
        };
        if let Err(err) = self.driver.submit(submission) {
            return UringOp::ready(Err(err));
        }
        UringOp::new(rx)
    }
}

/// Builder for Linux file-open flags used by [`UringRuntime::open`].
#[derive(Clone, Debug)]
pub struct UringOpenOptions {
    read: bool,
    write: bool,
    create: bool,
    create_new: bool,
    truncate: bool,
    append: bool,
    mode: u32,
    custom_flags: i32,
}

impl Default for UringOpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl UringOpenOptions {
    pub fn new() -> Self {
        Self {
            read: false,
            write: false,
            create: false,
            create_new: false,
            truncate: false,
            append: false,
            mode: 0o666,
            custom_flags: 0,
        }
    }

    pub fn read(mut self, enabled: bool) -> Self {
        self.read = enabled;
        self
    }

    pub fn write(mut self, enabled: bool) -> Self {
        self.write = enabled;
        self
    }

    pub fn create(mut self, enabled: bool) -> Self {
        self.create = enabled;
        self
    }

    pub fn create_new(mut self, enabled: bool) -> Self {
        self.create_new = enabled;
        self
    }

    pub fn truncate(mut self, enabled: bool) -> Self {
        self.truncate = enabled;
        self
    }

    pub fn append(mut self, enabled: bool) -> Self {
        self.append = enabled;
        self
    }

    pub fn mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    pub fn custom_flags(mut self, flags: i32) -> Self {
        self.custom_flags = flags;
        self
    }

    fn to_open_flags(&self) -> io::Result<(i32, libc::mode_t)> {
        let access_mode = match (self.read, self.write || self.append) {
            (true, true) => libc::O_RDWR,
            (false, true) => libc::O_WRONLY,
            _ => libc::O_RDONLY,
        };

        let mut flags = self.custom_flags | access_mode | libc::O_CLOEXEC;
        if self.create {
            flags |= libc::O_CREAT;
        }
        if self.create_new {
            flags |= libc::O_CREAT | libc::O_EXCL;
        }
        if self.truncate {
            flags |= libc::O_TRUNC;
        }
        if self.append {
            flags |= libc::O_APPEND;
        }

        let mode = self.mode.try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("mode {} does not fit into libc::mode_t", self.mode),
            )
        })?;

        Ok((flags, mode))
    }
}

/// Completion object for an in-flight file I/O operation.
///
/// Dropping this value detaches the operation. The underlying request remains
/// in the driver and any owned buffers are recycled when the CQE arrives.
pub struct UringOp<T> {
    inner: oneshot::Receiver<io::Result<T>>,
}

impl<T> fmt::Debug for UringOp<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UringOp(..)")
    }
}

impl<T> UringOp<T> {
    fn new(inner: oneshot::Receiver<io::Result<T>>) -> Self {
        Self { inner }
    }

    fn ready(result: io::Result<T>) -> Self {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(result);
        Self::new(rx)
    }
}

impl<T> Future for UringOp<T> {
    type Output = io::Result<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll(cx) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(_)) => Poll::Ready(Err(io::Error::other(
                "io_uring driver dropped completion before sending a result",
            ))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Reusable owned buffer leased from [`UringRuntime`].
pub struct OwnedBuf {
    pool: Arc<BufferPool>,
    storage: Vec<u8>,
}

impl fmt::Debug for OwnedBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedBuf")
            .field("len", &self.len())
            .field("capacity", &self.capacity())
            .finish()
    }
}

impl OwnedBuf {
    fn new(pool: Arc<BufferPool>, mut storage: Vec<u8>) -> Self {
        storage.clear();
        Self { pool, storage }
    }

    pub fn len(&self) -> usize {
        self.storage.len()
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.storage.capacity()
    }

    pub fn clear(&mut self) {
        self.storage.clear();
    }

    pub fn reserve(&mut self, additional: usize) {
        let required = self.len().saturating_add(additional);
        if self.capacity() >= required {
            return;
        }
        let rounded = round_capacity(required, self.pool.config.alignment);
        self.storage.reserve(rounded.saturating_sub(self.len()));
    }

    pub fn resize_zeroed(&mut self, len: usize) {
        self.reserve(len.saturating_sub(self.len()));
        self.storage.resize(len, 0);
    }

    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.storage.extend_from_slice(bytes);
    }

    pub fn truncate(&mut self, len: usize) {
        self.storage.truncate(len);
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.storage
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.storage
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.storage.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.storage.as_mut_ptr()
    }

    fn prepare_read_append(&mut self, additional: usize) -> *mut u8 {
        self.reserve(additional);
        unsafe { self.storage.as_mut_ptr().add(self.storage.len()) }
    }

    unsafe fn advance_len(&mut self, additional: usize) {
        unsafe { self.storage.set_len(self.storage.len().saturating_add(additional)) };
    }
}

impl Drop for OwnedBuf {
    fn drop(&mut self) {
        let storage = std::mem::take(&mut self.storage);
        self.pool.release(storage);
    }
}

/// Owned vector of leased buffers for gather writes.
#[derive(Debug, Default)]
pub struct OwnedBufVec {
    buffers: Vec<OwnedBuf>,
}

impl OwnedBufVec {
    pub fn new(buffers: Vec<OwnedBuf>) -> Self {
        Self { buffers }
    }

    pub fn push(&mut self, buf: OwnedBuf) {
        self.buffers.push(buf);
    }

    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    pub fn total_len(&self) -> usize {
        self.buffers.iter().map(OwnedBuf::len).sum()
    }

    pub fn as_slice(&self) -> &[OwnedBuf] {
        &self.buffers
    }

    pub fn into_vec(self) -> Vec<OwnedBuf> {
        self.buffers
    }

    fn to_iovecs(&self, skip_buffers: usize, skip_bytes: usize) -> io::Result<Vec<libc::iovec>> {
        let mut iovecs = Vec::with_capacity(self.buffers.len().saturating_sub(skip_buffers));
        for (index, buf) in self.buffers.iter().enumerate().skip(skip_buffers) {
            let slice = if index == skip_buffers {
                &buf.as_slice()[skip_bytes..]
            } else {
                buf.as_slice()
            };
            if slice.is_empty() {
                continue;
            }
            iovecs.push(libc::iovec {
                iov_base: slice.as_ptr().cast::<c_void>().cast_mut(),
                iov_len: slice.len(),
            });
        }
        Ok(iovecs)
    }
}

impl From<Vec<OwnedBuf>> for OwnedBufVec {
    fn from(buffers: Vec<OwnedBuf>) -> Self {
        Self::new(buffers)
    }
}

/// Summary of a completed explicit-offset write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompletedWrite {
    pub offset: u64,
    pub len: usize,
}

/// Offset-reserving adapter that gives sequential ergonomics on top of
/// explicit-offset primitives.
#[derive(Clone, Debug)]
pub struct UringAppender {
    file: UringFile,
    next_offset: Arc<AtomicU64>,
}

impl UringAppender {
    pub fn new(file: UringFile, next_offset: u64) -> Self {
        Self {
            file,
            next_offset: Arc::new(AtomicU64::new(next_offset)),
        }
    }

    pub fn reserve(&self, len: usize) -> u64 {
        self.next_offset.fetch_add(len as u64, Ordering::Relaxed)
    }

    pub async fn append(&self, bufs: OwnedBufVec) -> io::Result<(OwnedBufVec, CompletedWrite)> {
        let offset = self.reserve(bufs.total_len());
        self.file.write_all_at(offset, bufs).await
    }
}

#[derive(Debug)]
struct UringFileInner {
    fd: OwnedFd,
}

/// Shared file handle for explicit-offset operations.
#[derive(Clone)]
pub struct UringFile {
    runtime: UringRuntime,
    inner: Arc<UringFileInner>,
}

impl fmt::Debug for UringFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UringFile")
            .field("fd", &self.inner.fd.as_raw_fd())
            .finish_non_exhaustive()
    }
}

impl UringFile {
    fn from_owned_fd(runtime: UringRuntime, fd: OwnedFd) -> Self {
        Self {
            runtime,
            inner: Arc::new(UringFileInner { fd }),
        }
    }

    /// Build an offset-reserving sequential writer above this file.
    pub fn appender(&self, next_offset: u64) -> UringAppender {
        UringAppender::new(self.clone(), next_offset)
    }

    /// Access the runtime-global allocator that backs this file's owned
    /// buffers.
    pub fn runtime(&self) -> &UringRuntime {
        &self.runtime
    }

    /// Read up to `read_len` bytes starting at `offset` into `buf`.
    pub fn read_at(&self, offset: u64, mut buf: OwnedBuf, read_len: usize) -> UringOp<(OwnedBuf, usize)> {
        buf.clear();
        self.read_into_at(offset, buf, read_len)
    }

    /// Read exactly `read_len` bytes starting at `offset`.
    pub async fn read_exact_at(&self, offset: u64, mut buf: OwnedBuf, read_len: usize) -> io::Result<OwnedBuf> {
        buf.clear();
        let mut filled = 0usize;
        while filled < read_len {
            let (next_buf, read) = self
                .read_into_at(offset + filled as u64, buf, read_len - filled)
                .await?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("expected {read_len} bytes at offset {offset}, got {filled}"),
                ));
            }
            buf = next_buf;
            filled += read;
        }
        Ok(buf)
    }

    /// Write bytes from `buf` at `offset`, returning the same owned buffer on
    /// completion so the caller can reuse it.
    pub fn write_at(&self, offset: u64, buf: OwnedBuf) -> UringOp<(OwnedBuf, usize)> {
        if buf.is_empty() {
            return UringOp::ready(Ok((buf, 0)));
        }

        let fd = self.inner.fd.as_raw_fd();
        self.runtime.submit(move |complete| {
            let len = usize_to_u32(buf.len(), "write length")?;
            let sqe = sqe_write(fd, buf.as_ptr(), len, offset);
            Ok(DriverSubmission::new(
                sqe,
                CompletionResources::OwnedBuf(buf),
                Box::new(move |result, resources| {
                    let buf = resources.into_owned_buf();
                    let output = result.and_then(|cqe| Ok((buf, cqe_to_usize(cqe.result)?)));
                    let _ = complete.send(output);
                }),
            ))
        })
    }

    /// Submit a gather write at `offset`.
    pub fn writev_at(&self, offset: u64, bufs: OwnedBufVec) -> UringOp<(OwnedBufVec, usize)> {
        self.writev_at_inner(offset, bufs, 0, 0)
    }

    /// Write the full set of buffers starting at `offset`.
    pub async fn write_all_at(&self, offset: u64, bufs: OwnedBufVec) -> io::Result<(OwnedBufVec, CompletedWrite)> {
        let total = bufs.total_len();
        if total == 0 {
            return Ok((bufs, CompletedWrite { offset, len: 0 }));
        }

        let mut written = 0usize;
        let mut skip_buffers = 0usize;
        let mut skip_bytes = 0usize;
        let mut bufs = bufs;

        while written < total {
            let (next_bufs, just_written) = self
                .writev_at_inner(offset + written as u64, bufs, skip_buffers, skip_bytes)
                .await?;
            if just_written == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    format!("write_all_at made no progress after writing {written} of {total} bytes"),
                ));
            }
            bufs = next_bufs;
            written += just_written;
            (skip_buffers, skip_bytes) = advance_progress(bufs.as_slice(), skip_buffers, skip_bytes, just_written);
        }

        Ok((bufs, CompletedWrite { offset, len: written }))
    }

    /// Preallocate file space without changing the logical write contract.
    pub fn fallocate(&self, offset: u64, len: u64) -> UringOp<()> {
        let fd = self.inner.fd.as_raw_fd();
        self.runtime.submit(move |complete| {
            let sqe = sqe_fallocate(fd, len, offset, 0);
            Ok(unit_submission(sqe, CompletionResources::None, complete))
        })
    }

    /// Sync file data without forcing all metadata.
    pub fn fdatasync(&self) -> UringOp<()> {
        let fd = self.inner.fd.as_raw_fd();
        self.runtime.submit(move |complete| {
            let sqe = sqe_fsync(fd, IORING_FSYNC_DATASYNC);
            Ok(unit_submission(sqe, CompletionResources::None, complete))
        })
    }

    /// Sync file data and metadata.
    pub fn fsync(&self) -> UringOp<()> {
        let fd = self.inner.fd.as_raw_fd();
        self.runtime.submit(move |complete| {
            let sqe = sqe_fsync(fd, 0);
            Ok(unit_submission(sqe, CompletionResources::None, complete))
        })
    }

    /// Truncate or extend the file to `len` bytes.
    pub fn set_len(&self, len: u64) -> UringOp<()> {
        let fd = self.inner.fd.as_raw_fd();
        self.runtime.submit(move |complete| {
            let sqe = sqe_ftruncate(fd, len);
            Ok(unit_submission(sqe, CompletionResources::None, complete))
        })
    }

    fn read_into_at(&self, offset: u64, mut buf: OwnedBuf, read_len: usize) -> UringOp<(OwnedBuf, usize)> {
        if read_len == 0 {
            return UringOp::ready(Ok((buf, 0)));
        }

        let fd = self.inner.fd.as_raw_fd();
        self.runtime.submit(move |complete| {
            let len = usize_to_u32(read_len, "read length")?;
            let ptr = buf.prepare_read_append(read_len);
            let sqe = sqe_read(fd, ptr, len, offset);
            Ok(DriverSubmission::new(
                sqe,
                CompletionResources::OwnedBuf(buf),
                Box::new(move |result, resources| {
                    let mut buf = resources.into_owned_buf();
                    let output = result.and_then(|cqe| {
                        let read = cqe_to_usize(cqe.result)?;
                        unsafe { buf.advance_len(read) };
                        Ok((buf, read))
                    });
                    let _ = complete.send(output);
                }),
            ))
        })
    }

    fn writev_at_inner(
        &self,
        offset: u64,
        bufs: OwnedBufVec,
        skip_buffers: usize,
        skip_bytes: usize,
    ) -> UringOp<(OwnedBufVec, usize)> {
        if skip_buffers >= bufs.len() {
            return UringOp::ready(Ok((bufs, 0)));
        }

        let fd = self.inner.fd.as_raw_fd();
        self.runtime.submit(move |complete| {
            let iovecs = bufs.to_iovecs(skip_buffers, skip_bytes)?;
            if iovecs.is_empty() {
                return Ok(DriverSubmission::completed_now(
                    CompletionResources::OwnedBufVec { bufs, iovecs },
                    Box::new(move |resources| {
                        let bufs = resources.into_owned_buf_vec();
                        let _ = complete.send(Ok((bufs, 0)));
                    }),
                ));
            }

            let iovec_len = usize_to_u32(iovecs.len(), "iovec count")?;
            let sqe = sqe_writev(fd, iovecs.as_ptr(), iovec_len, offset);
            Ok(DriverSubmission::new(
                sqe,
                CompletionResources::OwnedBufVec { bufs, iovecs },
                Box::new(move |result, resources| {
                    let bufs = resources.into_owned_buf_vec();
                    let output = result.and_then(|cqe| Ok((bufs, cqe_to_usize(cqe.result)?)));
                    let _ = complete.send(output);
                }),
            ))
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BufferPoolConfig {
    pub alignment: usize,
    pub max_cached_bucket_len: usize,
    pub max_cached_buffer_capacity: usize,
}

impl Default for BufferPoolConfig {
    fn default() -> Self {
        Self {
            alignment: DEFAULT_BUFFER_ALIGNMENT,
            max_cached_bucket_len: DEFAULT_MAX_CACHED_BUCKET_LEN,
            max_cached_buffer_capacity: DEFAULT_MAX_CACHED_BUFFER_CAPACITY,
        }
    }
}

struct BufferPool {
    config: BufferPoolConfig,
    inner: Mutex<BTreeMap<usize, Vec<Vec<u8>>>>,
}

impl BufferPool {
    fn new(config: BufferPoolConfig) -> Self {
        Self {
            config,
            inner: Mutex::new(BTreeMap::new()),
        }
    }

    fn acquire(&self, capacity: usize) -> Vec<u8> {
        if capacity == 0 {
            return Vec::new();
        }

        let rounded = round_capacity(capacity, self.config.alignment);
        let mut guard = self.inner.lock().unwrap();
        let mut selected = None;
        for (bucket_capacity, bucket) in guard.range_mut(rounded..) {
            if let Some(buffer) = bucket.pop() {
                let should_remove = bucket.is_empty();
                selected = Some((*bucket_capacity, should_remove, buffer));
                break;
            }
        }
        if let Some((bucket_capacity, should_remove, mut buffer)) = selected {
            if should_remove {
                guard.remove(&bucket_capacity);
            }
            buffer.clear();
            return buffer;
        }

        Vec::with_capacity(rounded)
    }

    fn release(&self, mut storage: Vec<u8>) {
        let capacity = storage.capacity();
        if capacity == 0 || capacity > self.config.max_cached_buffer_capacity {
            return;
        }
        storage.clear();

        let mut guard = self.inner.lock().unwrap();
        let bucket = guard.entry(capacity).or_default();
        if bucket.len() < self.config.max_cached_bucket_len {
            bucket.push(storage);
        }
    }
}

type CompletionFn = Box<dyn FnOnce(io::Result<CqeResult>, CompletionResources) + Send + 'static>;
type ImmediateCompletionFn = Box<dyn FnOnce(CompletionResources) + Send + 'static>;

struct DriverSubmission {
    sqe: Option<IoUringSqe>,
    resources: CompletionResources,
    complete: Option<CompletionFn>,
    complete_now: Option<ImmediateCompletionFn>,
}

impl DriverSubmission {
    fn new(sqe: IoUringSqe, resources: CompletionResources, complete: CompletionFn) -> Self {
        Self {
            sqe: Some(sqe),
            resources,
            complete: Some(complete),
            complete_now: None,
        }
    }

    fn completed_now(resources: CompletionResources, complete_now: ImmediateCompletionFn) -> Self {
        Self {
            sqe: None,
            resources,
            complete: None,
            complete_now: Some(complete_now),
        }
    }
}

enum CompletionResources {
    None,
    Path(CString),
    PathPair(CString, CString),
    OwnedBuf(OwnedBuf),
    OwnedBufVec {
        bufs: OwnedBufVec,
        iovecs: Vec<libc::iovec>,
    },
}

impl CompletionResources {
    fn into_owned_buf(self) -> OwnedBuf {
        match self {
            Self::OwnedBuf(buf) => buf,
            _ => unreachable!("completion resources did not contain an owned buffer"),
        }
    }

    fn into_owned_buf_vec(self) -> OwnedBufVec {
        match self {
            Self::OwnedBufVec { bufs, .. } => bufs,
            _ => unreachable!("completion resources did not contain an owned buffer vector"),
        }
    }
}

struct InFlight {
    resources: CompletionResources,
    complete: CompletionFn,
}

#[derive(Clone)]
struct DriverHandle {
    tx: mpsc::Sender<DriverSubmission>,
}

impl DriverHandle {
    fn spawn() -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("spacetimedb-uring".to_owned())
            .spawn(move || {
                let thread = match DriverThread::new(rx) {
                    Ok(thread) => {
                        let _ = ready_tx.send(Ok(()));
                        thread
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                thread.run();
            })?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { tx }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(io::Error::other(
                "io_uring driver thread exited before initialization completed",
            )),
        }
    }

    fn submit(&self, submission: DriverSubmission) -> io::Result<()> {
        self.tx
            .send(submission)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "io_uring driver thread is not running"))
    }
}

struct DriverThread {
    ring: RawRing,
    rx: mpsc::Receiver<DriverSubmission>,
    inflight: BTreeMap<u64, InFlight>,
    next_user_data: u64,
    channel_closed: bool,
}

impl DriverThread {
    fn new(rx: mpsc::Receiver<DriverSubmission>) -> io::Result<Self> {
        Ok(Self {
            ring: RawRing::new(DEFAULT_RING_ENTRIES)?,
            rx,
            inflight: BTreeMap::new(),
            next_user_data: 1,
            channel_closed: false,
        })
    }

    fn run(mut self) {
        if let Err(err) = self.run_inner() {
            self.fail_all(err);
        }
    }

    fn run_inner(&mut self) -> io::Result<()> {
        loop {
            if self.channel_closed && self.inflight.is_empty() {
                return Ok(());
            }

            if self.inflight.is_empty() && !self.channel_closed {
                match self.rx.recv() {
                    Ok(submission) => self.queue_submission(submission)?,
                    Err(_) => {
                        self.channel_closed = true;
                        continue;
                    }
                }
            }

            self.process_immediate_requests()?;
            self.process_completions()?;

            if self.ring.has_pending_submissions() {
                self.ring.submit_pending()?;
                self.process_completions()?;
            }

            if !self.inflight.is_empty() {
                if self.process_completions()? == 0 {
                    self.ring.wait_for_completion()?;
                    self.process_completions()?;
                }
                continue;
            }

            if self.channel_closed {
                return Ok(());
            }
        }
    }

    fn process_immediate_requests(&mut self) -> io::Result<()> {
        loop {
            match self.rx.try_recv() {
                Ok(submission) => {
                    if let Some(complete_now) = submission.complete_now {
                        complete_now(submission.resources);
                        continue;
                    }
                    self.queue_submission(submission)?;
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.channel_closed = true;
                    return Ok(());
                }
            }
        }
    }

    fn queue_submission(&mut self, mut submission: DriverSubmission) -> io::Result<()> {
        if let Some(complete_now) = submission.complete_now.take() {
            complete_now(submission.resources);
            return Ok(());
        }

        while self.ring.is_submission_queue_full() {
            if self.ring.has_pending_submissions() {
                self.ring.submit_pending()?;
            }
            if self.process_completions()? == 0 {
                self.ring.wait_for_completion()?;
                self.process_completions()?;
            }
        }

        let mut sqe = submission
            .sqe
            .take()
            .expect("submission without SQE reached queue_submission");
        let user_data = self.allocate_user_data();
        sqe.user_data = user_data;
        self.ring.push_submission(sqe)?;
        self.inflight.insert(
            user_data,
            InFlight {
                resources: submission.resources,
                complete: submission.complete.take().expect("completion callback missing"),
            },
        );
        Ok(())
    }

    fn process_completions(&mut self) -> io::Result<usize> {
        let mut completed = 0usize;
        while let Some(cqe) = self.ring.pop_completion()? {
            completed += 1;
            let result = cqe_result(cqe.res, cqe.flags);
            if let Some(inflight) = self.inflight.remove(&cqe.user_data) {
                (inflight.complete)(result, inflight.resources);
            }
        }
        Ok(completed)
    }

    fn fail_all(&mut self, err: io::Error) {
        let message = err.to_string();
        let kind = err.kind();
        let raw = err.raw_os_error();
        let mut send_error = |complete: CompletionFn, resources: CompletionResources| {
            let err = raw
                .map(io::Error::from_raw_os_error)
                .unwrap_or_else(|| io::Error::new(kind, message.clone()));
            complete(Err(err), resources);
        };

        for (_, inflight) in std::mem::take(&mut self.inflight) {
            send_error(inflight.complete, inflight.resources);
        }

        while let Ok(mut submission) = self.rx.try_recv() {
            if let Some(complete_now) = submission.complete_now.take() {
                complete_now(submission.resources);
                continue;
            }
            if let Some(complete) = submission.complete.take() {
                send_error(complete, submission.resources);
            }
        }
    }

    fn allocate_user_data(&mut self) -> u64 {
        let user_data = self.next_user_data;
        self.next_user_data = self.next_user_data.wrapping_add(1);
        if self.next_user_data == 0 {
            self.next_user_data = 1;
        }
        user_data
    }
}

#[derive(Clone, Copy)]
struct CqeResult {
    result: i32,
    flags: u32,
}

struct RawRing {
    fd: OwnedFd,
    sq_ring_map: Mmap,
    cq_ring_map: Option<Mmap>,
    sqes_map: Mmap,
    sq: SubmissionRing,
    cq: CompletionRing,
    pending_submissions: u32,
}

impl RawRing {
    fn new(entries: u32) -> io::Result<Self> {
        let mut params = IoUringParams::default();
        let fd = io_uring_setup(entries, &mut params)?;
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        let sq_ring_size = params.sq_off.array as usize + params.sq_entries as usize * std::mem::size_of::<u32>();
        let cq_ring_size = params.cq_off.cqes as usize + params.cq_entries as usize * std::mem::size_of::<IoUringCqe>();
        let use_single_mmap = (params.features & IORING_FEAT_SINGLE_MMAP) != 0;

        let sq_map_len = if use_single_mmap {
            sq_ring_size.max(cq_ring_size)
        } else {
            sq_ring_size
        };
        let sq_ring_map = Mmap::map(fd.as_raw_fd(), IORING_OFF_SQ_RING, sq_map_len)?;
        let cq_ring_map = if use_single_mmap {
            None
        } else {
            Some(Mmap::map(fd.as_raw_fd(), IORING_OFF_CQ_RING, cq_ring_size)?)
        };
        let sqes_map = Mmap::map(
            fd.as_raw_fd(),
            IORING_OFF_SQES,
            params.sq_entries as usize * std::mem::size_of::<IoUringSqe>(),
        )?;

        let sq_base = sq_ring_map.ptr;
        let cq_base = cq_ring_map.as_ref().map(|map| map.ptr).unwrap_or(sq_ring_map.ptr);

        let sq = SubmissionRing::from_mmap(sq_base, sqes_map.ptr, &params);
        let cq = CompletionRing::from_mmap(cq_base, &params);

        Ok(Self {
            fd,
            sq_ring_map,
            cq_ring_map,
            sqes_map,
            sq,
            cq,
            pending_submissions: 0,
        })
    }

    fn has_pending_submissions(&self) -> bool {
        self.pending_submissions != 0
    }

    fn is_submission_queue_full(&self) -> bool {
        self.sq.is_full()
    }

    fn push_submission(&mut self, sqe: IoUringSqe) -> io::Result<()> {
        self.sq.push(sqe)?;
        self.pending_submissions = self.pending_submissions.saturating_add(1);
        Ok(())
    }

    fn submit_pending(&mut self) -> io::Result<()> {
        while self.pending_submissions != 0 {
            let submitted = io_uring_enter(self.fd.as_raw_fd(), self.pending_submissions, 0, 0)?;
            if submitted == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "io_uring_enter submitted zero SQEs",
                ));
            }
            self.pending_submissions = self.pending_submissions.saturating_sub(submitted);
        }
        Ok(())
    }

    fn wait_for_completion(&mut self) -> io::Result<()> {
        self.submit_pending()?;
        io_uring_enter(self.fd.as_raw_fd(), 0, 1, IORING_ENTER_GETEVENTS)?;
        Ok(())
    }

    fn pop_completion(&mut self) -> io::Result<Option<IoUringCqe>> {
        Ok(self.cq.pop())
    }
}

struct SubmissionRing {
    khead: *const AtomicU32,
    ktail: *const AtomicU32,
    ring_mask: *const u32,
    ring_entries: *const u32,
    array: *mut u32,
    sqes: *mut IoUringSqe,
    tail: u32,
}

impl SubmissionRing {
    fn from_mmap(base: *mut c_void, sqes: *mut c_void, params: &IoUringParams) -> Self {
        let khead = unsafe { offset_ptr::<AtomicU32>(base, params.sq_off.head) };
        let ktail = unsafe { offset_ptr::<AtomicU32>(base, params.sq_off.tail) };
        Self {
            khead,
            ktail,
            ring_mask: unsafe { offset_ptr(base, params.sq_off.ring_mask) },
            ring_entries: unsafe { offset_ptr(base, params.sq_off.ring_entries) },
            array: unsafe { offset_ptr(base, params.sq_off.array) },
            sqes: sqes.cast::<IoUringSqe>(),
            tail: unsafe { (*ktail).load(Ordering::Acquire) },
        }
    }

    fn is_full(&self) -> bool {
        let head = unsafe { (*self.khead).load(Ordering::Acquire) };
        self.tail.wrapping_sub(head) == unsafe { *self.ring_entries }
    }

    fn push(&mut self, sqe: IoUringSqe) -> io::Result<()> {
        if self.is_full() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "io_uring submission queue is full",
            ));
        }

        let mask = unsafe { *self.ring_mask };
        let index = self.tail & mask;
        unsafe {
            self.sqes.add(index as usize).write(sqe);
            self.array.add(index as usize).write(index);
            self.tail = self.tail.wrapping_add(1);
            (*self.ktail).store(self.tail, Ordering::Release);
        }
        Ok(())
    }
}

struct CompletionRing {
    khead: *const AtomicU32,
    ktail: *const AtomicU32,
    ring_mask: *const u32,
    cqes: *mut IoUringCqe,
}

impl CompletionRing {
    fn from_mmap(base: *mut c_void, params: &IoUringParams) -> Self {
        Self {
            khead: unsafe { offset_ptr::<AtomicU32>(base, params.cq_off.head) },
            ktail: unsafe { offset_ptr::<AtomicU32>(base, params.cq_off.tail) },
            ring_mask: unsafe { offset_ptr(base, params.cq_off.ring_mask) },
            cqes: unsafe { offset_ptr(base, params.cq_off.cqes) },
        }
    }

    fn pop(&mut self) -> Option<IoUringCqe> {
        let head = unsafe { (*self.khead).load(Ordering::Acquire) };
        let tail = unsafe { (*self.ktail).load(Ordering::Acquire) };
        if head == tail {
            return None;
        }

        let mask = unsafe { *self.ring_mask };
        let index = head & mask;
        let cqe = unsafe { self.cqes.add(index as usize).read() };
        unsafe {
            (*self.khead).store(head.wrapping_add(1), Ordering::Release);
        }
        Some(cqe)
    }
}

struct Mmap {
    ptr: *mut c_void,
    len: usize,
}

impl Mmap {
    fn map(fd: RawFd, offset: u64, len: usize) -> io::Result<Self> {
        let offset = u64_to_off_t(offset)?;
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                offset,
            )
        };
        if ptr == libc::MAP_FAILED {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self { ptr, len })
        }
    }
}

impl Drop for Mmap {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr, self.len);
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IoSqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    flags: u32,
    dropped: u32,
    array: u32,
    resv1: u32,
    user_addr: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IoCqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    overflow: u32,
    cqes: u32,
    flags: u32,
    resv1: u32,
    user_addr: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IoUringParams {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: IoSqringOffsets,
    cq_off: IoCqringOffsets,
}

#[repr(C)]
#[derive(Clone, Copy)]
union SqeAnon1 {
    off: u64,
    addr2: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
union SqeAnon2 {
    addr: u64,
    splice_off_in: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
union SqeAnon3 {
    rw_flags: u32,
    fsync_flags: u32,
    poll32_events: u32,
    sync_range_flags: u32,
    msg_flags: u32,
    timeout_flags: u32,
    cancel_flags: u32,
    open_flags: u32,
    statx_flags: u32,
    splice_flags: u32,
    rename_flags: u32,
    unlink_flags: u32,
    hardlink_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
union SqeAnon4 {
    buf_index: u16,
    buf_group: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
union SqeAnon5 {
    splice_fd_in: i32,
    file_index: u32,
    optlen: u32,
    addr_len: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
union SqeAnon6 {
    addr3: u64,
    pad2: [u64; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IoUringSqe {
    opcode: u8,
    flags: u8,
    ioprio: u16,
    fd: i32,
    anon1: SqeAnon1,
    anon2: SqeAnon2,
    len: u32,
    anon3: SqeAnon3,
    anon4: SqeAnon4,
    personality: u16,
    anon5: SqeAnon5,
    anon6: SqeAnon6,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IoUringCqe {
    user_data: u64,
    res: i32,
    flags: u32,
}

fn sqe_zeroed() -> IoUringSqe {
    unsafe { std::mem::zeroed() }
}

fn sqe_openat(dirfd: RawFd, pathname: *const libc::c_char, flags: i32, mode: libc::mode_t) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_OPENAT;
    sqe.fd = dirfd;
    sqe.anon2 = SqeAnon2 { addr: pathname as u64 };
    sqe.len = mode;
    sqe.anon3 = SqeAnon3 {
        open_flags: flags as u32,
    };
    sqe
}

fn sqe_mkdirat(dirfd: RawFd, pathname: *const libc::c_char, mode: libc::mode_t) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_MKDIRAT;
    sqe.fd = dirfd;
    sqe.anon2 = SqeAnon2 { addr: pathname as u64 };
    sqe.len = mode;
    sqe
}

fn sqe_renameat(
    olddirfd: RawFd,
    oldpath: *const libc::c_char,
    newdirfd: RawFd,
    newpath: *const libc::c_char,
    flags: u32,
) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_RENAMEAT;
    sqe.fd = olddirfd;
    sqe.anon2 = SqeAnon2 { addr: oldpath as u64 };
    sqe.len = newdirfd as u32;
    sqe.anon1 = SqeAnon1 { off: newpath as u64 };
    sqe.anon3 = SqeAnon3 { rename_flags: flags };
    sqe
}

fn sqe_linkat(
    olddirfd: RawFd,
    oldpath: *const libc::c_char,
    newdirfd: RawFd,
    newpath: *const libc::c_char,
    flags: i32,
) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_LINKAT;
    sqe.fd = olddirfd;
    sqe.anon2 = SqeAnon2 { addr: oldpath as u64 };
    sqe.len = newdirfd as u32;
    sqe.anon1 = SqeAnon1 { addr2: newpath as u64 };
    sqe.anon3 = SqeAnon3 {
        hardlink_flags: flags as u32,
    };
    sqe
}

fn sqe_unlinkat(dirfd: RawFd, pathname: *const libc::c_char, flags: i32) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_UNLINKAT;
    sqe.fd = dirfd;
    sqe.anon2 = SqeAnon2 { addr: pathname as u64 };
    sqe.anon3 = SqeAnon3 {
        unlink_flags: flags as u32,
    };
    sqe
}

fn sqe_read(fd: RawFd, buf: *mut u8, len: u32, offset: u64) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_READ;
    sqe.fd = fd;
    sqe.anon2 = SqeAnon2 { addr: buf as u64 };
    sqe.len = len;
    sqe.anon1 = SqeAnon1 { off: offset };
    sqe
}

fn sqe_write(fd: RawFd, buf: *const u8, len: u32, offset: u64) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_WRITE;
    sqe.fd = fd;
    sqe.anon2 = SqeAnon2 { addr: buf as u64 };
    sqe.len = len;
    sqe.anon1 = SqeAnon1 { off: offset };
    sqe
}

fn sqe_writev(fd: RawFd, iovecs: *const libc::iovec, len: u32, offset: u64) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_WRITEV;
    sqe.fd = fd;
    sqe.anon2 = SqeAnon2 { addr: iovecs as u64 };
    sqe.len = len;
    sqe.anon1 = SqeAnon1 { off: offset };
    sqe
}

fn sqe_fsync(fd: RawFd, flags: u32) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_FSYNC;
    sqe.fd = fd;
    sqe.anon3 = SqeAnon3 { fsync_flags: flags };
    sqe
}

fn sqe_fallocate(fd: RawFd, len: u64, offset: u64, mode: i32) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_FALLOCATE;
    sqe.fd = fd;
    sqe.anon2 = SqeAnon2 { addr: len };
    sqe.len = mode as u32;
    sqe.anon1 = SqeAnon1 { off: offset };
    sqe
}

fn sqe_ftruncate(fd: RawFd, len: u64) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_FTRUNCATE;
    sqe.fd = fd;
    sqe.anon1 = SqeAnon1 { off: len };
    sqe
}

#[allow(dead_code)]
fn sqe_close(fd: RawFd) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_CLOSE;
    sqe.fd = fd;
    sqe
}

#[allow(dead_code)]
fn sqe_async_cancel(user_data: u64) -> IoUringSqe {
    let mut sqe = sqe_zeroed();
    sqe.opcode = IORING_OP_ASYNC_CANCEL;
    sqe.fd = -1;
    sqe.anon2 = SqeAnon2 { addr: user_data };
    sqe
}

fn unit_submission(
    sqe: IoUringSqe,
    resources: CompletionResources,
    complete: oneshot::Sender<io::Result<()>>,
) -> DriverSubmission {
    DriverSubmission::new(
        sqe,
        resources,
        Box::new(move |result, _resources| {
            let output = result.map(|_| ());
            let _ = complete.send(output);
        }),
    )
}

fn path_to_cstring(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path contains an interior NUL byte: {}", path.display()),
        )
    })
}

fn cqe_result(result: i32, flags: u32) -> io::Result<CqeResult> {
    if result < 0 {
        Err(io::Error::from_raw_os_error(-result))
    } else {
        Ok(CqeResult { result, flags })
    }
}

fn cqe_to_usize(value: i32) -> io::Result<usize> {
    value.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("completion result {value} does not fit into usize"),
        )
    })
}

fn io_uring_setup(entries: u32, params: &mut IoUringParams) -> io::Result<RawFd> {
    let result = unsafe {
        libc::syscall(
            libc::SYS_io_uring_setup as libc::c_long,
            entries,
            params as *mut IoUringParams,
        )
    };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result as RawFd)
    }
}

fn io_uring_enter(fd: RawFd, to_submit: u32, min_complete: u32, flags: u32) -> io::Result<u32> {
    let result = unsafe {
        libc::syscall(
            libc::SYS_io_uring_enter as libc::c_long,
            fd,
            to_submit,
            min_complete,
            flags,
            ptr::null::<libc::sigset_t>(),
            0usize,
        )
    };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result as u32)
    }
}

fn global_driver() -> io::Result<Arc<DriverHandle>> {
    static DRIVER: OnceLock<Result<Arc<DriverHandle>, String>> = OnceLock::new();
    match DRIVER.get_or_init(|| {
        DriverHandle::spawn()
            .map(Arc::new)
            .map_err(|err| format!("failed to initialize io_uring driver: {err}"))
    }) {
        Ok(handle) => Ok(handle.clone()),
        Err(err) => Err(io::Error::other(err.clone())),
    }
}

unsafe fn offset_ptr<T>(base: *mut c_void, offset: u32) -> *mut T {
    unsafe { base.cast::<u8>().add(offset as usize).cast::<T>() }
}

fn round_capacity(capacity: usize, alignment: usize) -> usize {
    if capacity == 0 {
        return 0;
    }
    if alignment <= 1 {
        return capacity;
    }
    capacity.next_multiple_of(alignment)
}

fn advance_progress(
    bufs: &[OwnedBuf],
    mut buf_index: usize,
    mut buf_offset: usize,
    mut consumed: usize,
) -> (usize, usize) {
    while consumed > 0 {
        let remaining = bufs[buf_index].len() - buf_offset;
        if consumed < remaining {
            buf_offset += consumed;
            return (buf_index, buf_offset);
        }
        consumed -= remaining;
        buf_index += 1;
        buf_offset = 0;
    }
    (buf_index, buf_offset)
}

fn usize_to_u32(value: usize, label: &str) -> io::Result<u32> {
    value.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} {value} does not fit into u32"),
        )
    })
}

fn u64_to_off_t(value: u64) -> io::Result<libc::off_t> {
    value.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("value {value} does not fit into libc::off_t"),
        )
    })
}
