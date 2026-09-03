use alloc::{boxed::Box, sync::Arc};
use core::{
    pin::Pin,
    result::Result,
    task::{Context, Poll},
    time::Duration,
};
use futures_channel::oneshot;
use slab::Slab;

use crate::{
    io::{AlignedBytes, ErasedBox, ErrorWith, SpacetimeIO, Statx},
    sim::{io::executor::FaultInjector, Rng},
};

mod executor;
use executor::{Cqe, Executor, Sqe};

mod fs;
pub use fs::File;

pub use crate::io::SECTOR_SIZE;

/// Simulated clock measurement.
///
/// In simulated time, an instant is actually a [Duration] since the time
/// instance was instantiated. To avoid confusion, we use the name "instant" to
/// convey that its semantics are that of the standard library type of the same
/// name.
pub type Instant = Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("file not found")]
    FileNotFound { path: Box<str> },
    #[error("file already exists")]
    FileAlreadyExists { path: Box<str> },
    #[error("failed to write expected number of bytes")]
    ShortWrite { expected: usize, written: usize },
    #[error("unexpected eof")]
    UnexpectedEof { expected: usize, read: usize },
    #[error(transparent)]
    Fs(fs::Error),
    /// Injected by the I/O driver.
    #[error("operation cancelled")]
    Cancelled,
    #[error("submission queue overflow")]
    SubmissionQueueOverflow,
}

impl From<fs::Error> for Error {
    fn from(e: fs::Error) -> Self {
        Self::Fs(e)
    }
}

#[derive(Clone, Default)]
pub struct SimulatorIO {
    inner: Arc<SimulatorInner>,
}

impl SimulatorIO {
    pub fn tick(&self, rng: &Rng, faults: &mut impl FaultInjector<usize>) -> bool {
        let mut executor = self.inner.executor.lock();
        let mut pending = self.inner.pending.lock();
        let mut buffers = self.inner.buffers.lock();

        let mut progress = executor.tick(rng, faults);
        for cqe in executor.completed() {
            let completion = pending.remove(cqe.user_data().unwrap());
            match cqe {
                Cqe::Write { result, .. } => {
                    let CompletionHandle::Write { tx, buf_key } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let erased_buf = buffers.remove(buf_key);
                    let result = match result {
                        Ok(_written) => Ok(erased_buf),
                        Err(error) => Err(ErrorWith {
                            error,
                            with: erased_buf,
                        }),
                    };
                    let _ = tx.send(result);
                }
                Cqe::Read { result, .. } => {
                    let CompletionHandle::Read { tx, buf_key } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let erased_buf = buffers.remove(buf_key);
                    let result = match result {
                        Ok(_written) => Ok(erased_buf),
                        Err(error) => Err(ErrorWith {
                            error,
                            with: erased_buf,
                        }),
                    };
                    let _ = tx.send(result);
                }
                Cqe::Open { result, .. } => {
                    let CompletionHandle::Open { tx } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let _ = tx.send(result);
                }
                Cqe::Create { result, .. } => {
                    let CompletionHandle::Create { tx } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let _ = tx.send(result);
                }
                Cqe::Stat { result, .. } => {
                    let CompletionHandle::Stat { tx } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let _ = tx.send(result);
                }
                Cqe::Fallocate { result, .. } => {
                    let CompletionHandle::Fallocate { tx } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let _ = tx.send(result);
                }
                Cqe::Fsync { result, .. } => {
                    let CompletionHandle::Fsync { tx } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let _ = tx.send(result);
                }
                Cqe::Fdatasync { result, .. } => {
                    let CompletionHandle::Fdatasync { tx } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let _ = tx.send(result);
                }
                Cqe::Noop { result, .. } => {
                    let CompletionHandle::Noop { tx } = completion else {
                        unreachable!("invalid cqe / completion pairing")
                    };
                    let _ = tx.send(result);
                }
            }

            progress |= true;
        }

        progress
    }

    fn submit<T>(
        &self,
        sqe: Sqe<usize>,
        completion_handle: impl FnOnce(CompletionSender<T, Error>) -> CompletionHandle,
    ) -> Completion<Result<T, Error>> {
        let (tx, rx) = oneshot::channel();

        let mut executor = self.inner.executor.lock();
        let mut pending = self.inner.pending.lock();
        let pending_entry = pending.vacant_entry();

        match executor.submit([sqe.attach(pending_entry.key())]) {
            Err(_sqe) => tx
                .send(Err(Error::SubmissionQueueOverflow))
                .unwrap_or_else(|_| unreachable!("rx is alive")),
            Ok(()) => {
                pending_entry.insert(completion_handle(tx));
            }
        }

        rx.into()
    }

    fn submit_with<B: AlignedBytes + 'static>(
        &self,
        sqe: Sqe<usize>,
        buf: ErasedBox,
        completion_handle: impl FnOnce(CompletionSender<ErasedBox, ErrorWith<Error, ErasedBox>>, usize) -> CompletionHandle,
    ) -> Completion<Result<B, ErrorWith<Error, B>>> {
        let (tx, rx) = oneshot::channel();

        let mut executor = self.inner.executor.lock();
        let mut pending = self.inner.pending.lock();
        let pending_entry = pending.vacant_entry();

        match executor.submit([sqe.attach(pending_entry.key())]) {
            Err(_sqe) => tx
                .send(Err(ErrorWith {
                    error: Error::SubmissionQueueOverflow,
                    with: buf,
                }))
                .unwrap_or_else(|_| unreachable!("rx is alive")),
            Ok(()) => {
                let buf_key = self.inner.buffers.lock().insert(buf);
                pending_entry.insert(completion_handle(tx, buf_key));
            }
        }

        Completion::mapped(rx, reify)
    }
}

struct SimulatorInner {
    executor: spin::Mutex<Executor<usize>>,
    pending: spin::Mutex<Slab<CompletionHandle>>,
    buffers: Arc<spin::Mutex<Slab<ErasedBox>>>,
}

impl Default for SimulatorInner {
    fn default() -> Self {
        Self {
            executor: spin::Mutex::new(Executor::new(<_>::default())),
            pending: <_>::default(),
            buffers: <_>::default(),
        }
    }
}

pub type CompletionReceiver<T, E> = oneshot::Receiver<Result<T, E>>;

#[must_use = "completions must be polled to completion"]
pub struct Completion<T>(CompletionInner<T>);

impl<T> Completion<T> {
    pub fn mapped(
        rx: CompletionReceiver<ErasedBox, ErrorWith<Error, ErasedBox>>,
        map: fn(Result<ErasedBox, ErrorWith<Error, ErasedBox>>) -> T,
    ) -> Self {
        Self(CompletionInner::Mapped { rx, map })
    }
}

impl<T> From<oneshot::Receiver<T>> for Completion<T> {
    fn from(rx: oneshot::Receiver<T>) -> Self {
        Self(CompletionInner::Direct { rx })
    }
}

impl<T> Future for Completion<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        Pin::new(&mut this.0).poll(cx)
    }
}

enum CompletionInner<T> {
    Direct {
        rx: oneshot::Receiver<T>,
    },
    Mapped {
        rx: CompletionReceiver<ErasedBox, ErrorWith<Error, ErasedBox>>,
        map: fn(Result<ErasedBox, ErrorWith<Error, ErasedBox>>) -> T,
    },
}

impl<T> Future for CompletionInner<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this {
            Self::Direct { rx } => Pin::new(rx)
                .poll(cx)
                .map(|result| result.expect("lost completion sender")),
            Self::Mapped { rx, map } => Pin::new(rx).poll(cx).map(|result| {
                let result = result.expect("lost completion sender");
                map(result)
            }),
        }
    }
}

type CompletionSender<T, E> = oneshot::Sender<Result<T, E>>;
enum CompletionHandle {
    Write {
        tx: CompletionSender<ErasedBox, ErrorWith<Error, ErasedBox>>,
        buf_key: usize,
    },
    Read {
        tx: CompletionSender<ErasedBox, ErrorWith<Error, ErasedBox>>,
        buf_key: usize,
    },
    Open {
        tx: CompletionSender<fs::File, Error>,
    },
    Create {
        tx: CompletionSender<fs::File, Error>,
    },
    Stat {
        tx: CompletionSender<Statx, Error>,
    },
    Fallocate {
        tx: CompletionSender<(), Error>,
    },
    Fsync {
        tx: CompletionSender<(), Error>,
    },
    Fdatasync {
        tx: CompletionSender<(), Error>,
    },
    // TODO: We may use this for timeouts.
    #[allow(unused)]
    Noop {
        tx: CompletionSender<(), Error>,
    },
}

impl SpacetimeIO for SimulatorIO {
    type Fd = fs::File;
    type Error = Error;
    type Completion<T> = Completion<T>;

    fn open_file(&self, path: &str) -> Self::Completion<Result<Self::Fd, Self::Error>> {
        self.submit(Sqe::open(path), |tx| CompletionHandle::Open { tx })
    }

    fn create_file(&self, path: &str) -> Self::Completion<Result<Self::Fd, Self::Error>> {
        self.submit(Sqe::create(path), |tx| CompletionHandle::Create { tx })
    }

    fn write_all_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>> {
        let erased_buf = ErasedBox::from_aligned(buf);
        let buf_ptr = erased_buf.as_ptr();
        self.submit_with(Sqe::write(fd, buf_ptr, offset), erased_buf, |tx, buf_key| {
            CompletionHandle::Write { tx, buf_key }
        })
    }

    fn read_exact_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>> {
        let erased_buf = ErasedBox::from_aligned(buf);
        let buf_ptr = erased_buf.as_ptr();
        self.submit_with(Sqe::read(fd, buf_ptr, offset), erased_buf, |tx, buf_key| {
            CompletionHandle::Read { tx, buf_key }
        })
    }

    fn fsync(&self, fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>> {
        self.submit(Sqe::fsync(fd), |tx| CompletionHandle::Fsync { tx })
    }

    fn fdatasync(&self, fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>> {
        self.submit(Sqe::fdatasync(fd), |tx| CompletionHandle::Fdatasync { tx })
    }

    fn reserve(&self, fd: Self::Fd, total_size: u64) -> Self::Completion<Result<(), Self::Error>> {
        self.submit(Sqe::fallocate(fd, total_size), |tx| CompletionHandle::Fallocate { tx })
    }

    fn statx(&self, fd: Self::Fd) -> Self::Completion<Result<Statx, Self::Error>> {
        self.submit(Sqe::stat(fd), |tx| CompletionHandle::Stat { tx })
    }
}

fn reify<T: AlignedBytes + 'static>(
    result: Result<ErasedBox, ErrorWith<Error, ErasedBox>>,
) -> Result<T, ErrorWith<Error, T>> {
    match result {
        Ok(erased) => Ok(erased.into_aligned::<T>()),
        Err(ErrorWith { error, with }) => Err(ErrorWith {
            error,
            with: with.into_aligned::<T>(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::sim::{io::executor::NoFaults, GlobalRng};

    use super::*;

    struct Runtime {
        rt: tokio::runtime::LocalRuntime,
        io: SimulatorIO,
        rng: Rng,
    }

    impl Runtime {
        fn new() -> Self {
            Self {
                rt: tokio::runtime::Builder::new_current_thread()
                    .build_local(<_>::default())
                    .unwrap(),
                io: SimulatorIO::default(),
                rng: GlobalRng::new(0),
            }
        }

        fn run<T: 'static>(&self, f: impl FnOnce(&SimulatorIO) -> Completion<T>) -> T {
            let fut = self.rt.spawn_local(f(&self.io));
            while self.io.tick(&self.rng, &mut NoFaults) {}
            self.rt.block_on(fut).unwrap()
        }
    }

    #[test]
    fn create_file() {
        let rt = Runtime::new();
        rt.run(|io| io.create_file("/data/test")).unwrap();
    }

    #[derive(Debug)]
    #[repr(C, align(4096))]
    struct Buf([u8; 2 * SECTOR_SIZE]);

    impl Buf {
        fn clear(&mut self) {
            self.0.fill(0);
        }
    }

    impl AlignedBytes for Buf {
        fn as_bytes(&self) -> &[u8] {
            &self.0
        }

        fn as_bytes_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }

        fn from_bytes(b: &[u8]) -> Self {
            assert_eq!(b.len(), 2 * SECTOR_SIZE);
            let mut buf = [0; 2 * SECTOR_SIZE];
            buf.copy_from_slice(b);
            Self(buf)
        }
    }

    #[test]
    fn write_read_roundtrip() {
        let rt = Runtime::new();

        let fd = rt.run(|io| io.create_file("/data/test")).unwrap();
        let mut buf = rt
            .run(|io| io.write_all_at(fd.clone(), Buf([22; 2 * SECTOR_SIZE]), 0))
            .unwrap();
        buf.clear();
        let buf = rt.run(|io| io.read_exact_at(fd, buf, 0)).unwrap();

        assert!(buf.0.iter().all(|&b| b == 22));
    }
}
