use std::io::{Seek, SeekFrom};
use std::panic;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{io, marker::PhantomData, rc::Rc, sync::Arc};

use spacetimedb_runtime_core::io::{AlignedBytes, ErrorWith, SpacetimeIO};
use static_assertions::assert_not_impl_any;
use tokio::runtime;

/// Implementation of [SpacetimeIO] that runs on a tokio runtime.
pub struct TokioIO {
    rt: runtime::Handle,
    // Ensure I/O stays on a single thread.
    _not_send: PhantomData<Rc<()>>,
}

impl TokioIO {
    pub fn new(rt: runtime::Handle) -> Self {
        Self {
            rt,
            _not_send: PhantomData,
        }
    }
}

assert_not_impl_any!(TokioIO: Send);

#[must_use = "completions must be polled to completion"]
pub struct Completion<T>(tokio::task::JoinHandle<T>);

impl<T> Future for Completion<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match Pin::new(&mut this.0).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => match result {
                Ok(output) => Poll::Ready(output),
                Err(error) => {
                    if error.is_panic() {
                        panic::resume_unwind(error.into_panic())
                    } else if error.is_cancelled() {
                        panic!("I/O task unexpectedly cancelled");
                    } else {
                        unreachable!("unexpected I/O task error")
                    }
                }
            },
        }
    }
}

impl<T> From<tokio::task::JoinHandle<T>> for Completion<T> {
    fn from(handle: tokio::task::JoinHandle<T>) -> Self {
        Self(handle)
    }
}

impl SpacetimeIO for TokioIO {
    // NOTE: This operates on a [std::fs::File] handle instead of
    // [tokio::fs::File] because `pwrite`/`pread`-style APIs are not available
    // from tokio proper. As a consequence, operations on an open `Fd` use
    // [spawn_blocking]. This is what [tokio::fs::File] does internally, while
    // here we can avoid some locking.
    type Fd = Arc<std::fs::File>;
    type Error = io::Error;
    type Completion<T> = Completion<T>;

    fn open_file(&self, path: &str) -> Self::Completion<Result<Self::Fd, Self::Error>> {
        let path = PathBuf::from(path);
        self.rt
            .spawn_blocking(move || {
                let mut open_options = std::fs::File::options();
                open_options.read(true).write(true);
                platform::open_with_direct_io(open_options, path).map(Arc::new)
            })
            .into()
    }

    fn create_file(&self, path: &str, len: u64) -> Self::Completion<Result<Self::Fd, Self::Error>> {
        let path = PathBuf::from(path);
        self.rt
            .spawn_blocking(move || {
                let mut open_options = std::fs::File::options();
                open_options.read(true).write(true).create_new(true);
                let file = platform::open_with_direct_io(open_options, path)?;
                file.set_len(len)?;
                Ok(Arc::new(file))
            })
            .into()
    }

    fn write_all_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>> {
        self.rt
            .spawn_blocking(move || match platform::write_all_at(&fd, buf.as_bytes(), offset) {
                Ok(()) => Ok(buf),
                Err(error) => Err(ErrorWith { error, with: buf }),
            })
            .into()
    }

    fn read_exact_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        mut buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>> {
        self.rt
            .spawn_blocking(move || match platform::read_exact_at(&fd, buf.as_bytes_mut(), offset) {
                Ok(()) => Ok(buf),
                Err(error) => Err(ErrorWith { error, with: buf }),
            })
            .into()
    }

    fn fsync(&self, fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>> {
        self.rt.spawn_blocking(move || fd.sync_all()).into()
    }

    fn fdatasync(&self, fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>> {
        self.rt.spawn_blocking(move || fd.sync_data()).into()
    }

    fn reserve(&self, fd: Self::Fd, total: u64) -> Self::Completion<Result<(), Self::Error>> {
        self.rt
            .spawn_blocking(move || {
                let mut fd = fd.try_clone()?;
                let len = file_length(&mut fd)?;
                assert!(total >= len);
                fd.set_len(total)
            })
            .into()
    }

    fn length(&self, fd: Self::Fd) -> Self::Completion<Result<u64, Self::Error>> {
        self.rt
            .spawn_blocking(move || {
                let mut fd = fd.try_clone()?;
                file_length(&mut fd)
            })
            .into()
    }
}

fn file_length(fd: &mut std::fs::File) -> io::Result<u64> {
    let pos = fd.stream_position()?;
    let len = fd.seek(SeekFrom::End(0))?;

    if pos != len {
        fd.seek(SeekFrom::Start(pos))?;
    }

    Ok(len)
}

mod platform {
    #[cfg(unix)]
    pub use super::unix::*;

    #[cfg(windows)]
    pub use super::windows::*;
}

#[cfg(unix)]
mod unix {
    use std::{
        io,
        os::unix::fs::{FileExt as _, OpenOptionsExt as _},
        path::Path,
    };

    #[inline]
    pub fn read_exact_at(fd: &std::fs::File, buf: &mut [u8], offset: u64) -> io::Result<()> {
        fd.read_exact_at(buf, offset)
    }

    #[inline]
    pub fn write_all_at(fd: &std::fs::File, buf: &[u8], offset: u64) -> io::Result<()> {
        fd.write_all_at(buf, offset)
    }

    #[cfg(not(target_os = "macos"))]
    pub fn open_with_direct_io(mut options: std::fs::OpenOptions, path: impl AsRef<Path>) -> io::Result<std::fs::File> {
        options.custom_flags(libc::O_DIRECT).open(path)
    }

    #[cfg(target_os = "macos")]
    pub async fn open_with_direct_io(
        options: std::fs::OpenOptions,
        path: impl AsRef<Path>,
    ) -> io::Result<std::fs::File> {
        let file = options.open(path)?;
        let res = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1) };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(file)
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::io;

    pub fn write_all_at(fd: &std::fs::File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
        while !buf.is_empty() {
            match fd.seek_write(buf, offset) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => {
                    offset += n as u64;
                    buf = &buf[n..];
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    pub fn read_exact_at(fd: &std::fs::File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
        while !buf.is_empty() {
            match fd.seek_read(buf, offset) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => {
                    offset += n as u64;
                    buf = &mut buf[n..];
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    pub async fn open_with_direct_io(
        mut options: std::fs::OpenOptions,
        path: impl AsRef<Path>,
    ) -> io::Result<std::fs::File> {
        use std::os::windows::fs::OpenOptionsExt as _;

        options
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_NO_BUFFERING)
            .open(path)
    }
}
