use std::{io, marker::PhantomData, rc::Rc, sync::Arc};

#[cfg(unix)]
use std::os::unix::fs::FileExt as _;
#[cfg(windows)]
use std::os::windows::fs::FileExt as _;

use spacetimedb_io_types::{AlignedBytes, ErrorWith, SpacetimeIO};
use static_assertions::assert_not_impl_any;
use tokio::{fs::OpenOptions, runtime, task::spawn_blocking};

/// Implementation of [SpacetimeIO] that runs on a tokio runtime.
pub struct TokioIO {
    // TODO: Should this be [runtime::Runtime]?
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

impl SpacetimeIO for TokioIO {
    // NOTE: This operates on a [std::fs::File] handle instead of
    // [tokio::fs::File] because `pwrite`/`pread`-style APIs are not available
    // from tokio proper. As a consequence, operations on an open `Fd` use
    // [spawn_blocking]. This is what [tokio::fs::File] does internally, while
    // here we can avoid some locking.
    type Fd = Arc<std::fs::File>;
    type Error = io::Error;

    async fn open_file(&self, path: &str) -> Result<Self::Fd, Self::Error> {
        let _rt = self.rt.enter();

        let mut open_options = tokio::fs::File::options();
        open_options.read(true).write(true);
        let file = open_with_direct_io(open_options, path).await?;

        Ok(Arc::new(file.into_std().await))
    }

    async fn create_file(&self, path: &str, len: u64) -> Result<Self::Fd, Self::Error> {
        let _rt = self.rt.enter();

        let mut open_options = tokio::fs::File::options();
        open_options.read(true).write(true).create_new(true);
        let file = open_with_direct_io(open_options, path).await?;
        file.set_len(len).await?;

        Ok(Arc::new(file.into_std().await))
    }

    async fn write_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Result<B, ErrorWith<Self::Error, B>> {
        let _rt = self.rt.enter();
        asyncify(move || {
            #[cfg(unix)]
            let res = fd.write_all_at(buf.as_bytes(), offset);
            #[cfg(windows)]
            let res = fd.seek_write(buf.as_bytes(), offset);

            match res {
                Ok(()) => Ok(buf),
                Err(error) => Err(ErrorWith { error, with: buf }),
            }
        })
        .await
    }

    async fn read_exact_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        mut buf: B,
        offset: u64,
    ) -> Result<B, ErrorWith<Self::Error, B>> {
        let _rt = self.rt.enter();
        asyncify(move || {
            #[cfg(unix)]
            let res = fd.read_exact_at(buf.as_bytes_mut(), offset);
            #[cfg(windows)]
            let res = fd.seek_read(buf.as_bytes_mut(), offset);

            match res {
                Ok(()) => Ok(buf),
                Err(error) => Err(ErrorWith { error, with: buf }),
            }
        })
        .await
    }

    async fn fsync(&self, fd: Self::Fd) -> Result<(), Self::Error> {
        let _rt = self.rt.enter();
        asyncify(move || fd.sync_all()).await
    }

    async fn fdatasync(&self, fd: Self::Fd) -> Result<(), Self::Error> {
        let _rt = self.rt.enter();
        asyncify(move || fd.sync_data()).await
    }

    async fn reserve(&self, fd: Self::Fd, additional: u64) -> Result<(), Self::Error> {
        let _rt = self.rt.enter();
        asyncify(move || {
            let len = fd.metadata()?.len();
            fd.set_len(len + additional)?;

            Ok(())
        })
        .await
    }
}

async fn asyncify<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    spawn_blocking(f).await.unwrap_or_else(|e| match e.try_into_panic() {
        Ok(panic_payload) => std::panic::resume_unwind(panic_payload),
        // A cancellation should not be possible, because we await the task.
        Err(e) => panic!("unexpected error joining blocking task: {e}"),
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
async fn open_with_direct_io(mut options: OpenOptions, path: &str) -> io::Result<tokio::fs::File> {
    options.custom_flags(libc::O_DIRECT).open(path).await
}

#[cfg(target_os = "macos")]
async fn open_with_direct_io(options: OpenOptions, path: &str) -> io::Result<tokio::fs::File> {
    let file = options.open(path).await?;
    asyncify(move || {
        let res = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1) };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(file)
        }
    })
    .await
}

#[cfg(target_os = "windows")]
async fn open_with_direct_io(options: OpenOptions, path: &str) -> io::Result<tokio::fs::File> {
    options
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_NO_BUFFERING)
        .open(path)
        .await
}
