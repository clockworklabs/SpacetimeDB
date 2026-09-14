use alloc::{boxed::Box, sync::Arc};
use core::result::Result;

use crate::{
    sim::completion::{CompletionState, PendingCompletions},
    AlignedBytes, ErasedBox, ErrorWith, SpacetimeIO, Statx,
};

mod completion;
pub use completion::Completion;
use completion::CompletionHandle;

mod executor;
use executor::{Executor, Sqe};

mod fs;
pub use fs::File;

pub use crate::{
    sim::executor::{FaultInjector, TaskSelector},
    SECTOR_SIZE,
};

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
    pub fn tick(&self, task_selector: &impl TaskSelector, faults: &mut impl FaultInjector<usize>) -> bool {
        let mut executor = self.inner.executor.lock();

        let mut progress = executor.tick(task_selector, faults);
        executor
            .completed()
            .map(|cqe| {
                let key = cqe.user_data().expect("user data must be set");
                let mut pending = self.inner.pending.lock();
                // If the handle is no longer present in `pending`, the
                // completion future was dropped.
                let handle = pending.get_mut(key)?;
                cqe.complete(handle)
            })
            .for_each(|waker| {
                if let Some(waker) = waker {
                    waker.wake();
                }
                progress |= true
            });

        progress
    }

    /// Simulate a power loss event.
    ///
    /// All submitted and executing operations are cancelled, and files reset to
    /// their durable state. Completions that have not been signalled will be
    /// dropped, too.
    pub fn power_loss(&self) {
        self.inner.executor.lock().power_loss();
        self.inner.pending.lock().clear();
    }

    /// Simulate a restart event, i.e. process crash.
    ///
    /// Unlike [Self::power_loss], this will drive the currently executing
    /// operations to completion, subject to fault injection.
    ///
    /// Submissions that were not yet scheduled are dropped. The file state
    /// remains unchanged.
    ///
    /// Completions that were not signalled during shutdown are dropped.
    pub fn restart(&self, faults: &mut impl FaultInjector<usize>) {
        self.inner.executor.lock().restart(faults);
        self.inner.pending.lock().clear();
    }

    fn submit<T, U>(
        &self,
        sqe: Sqe<usize>,
        completion: impl FnOnce(Arc<SimulatorInner>, usize) -> Completion<U>,
        completion_handle: impl FnOnce(CompletionState<Result<T, Error>>) -> CompletionHandle,
    ) -> Completion<U> {
        let mut executor = self.inner.executor.lock();
        let mut pending = self.inner.pending.lock();
        let pending_entry = pending.vacant_entry();

        let mut state = CompletionState::Pending(None);
        if let Err(_sqe) = executor.submit([sqe.attach(pending_entry.key())]) {
            state = CompletionState::Ready(Err(Error::SubmissionQueueOverflow));
        }
        let key = pending_entry.key();
        pending_entry.insert(completion_handle(state));

        completion(self.inner.clone(), key)
    }

    fn submit_with<B: AlignedBytes + 'static>(
        &self,
        sqe: Sqe<usize>,
        completion: impl FnOnce(Arc<SimulatorInner>, usize) -> Completion<Result<B, ErrorWith<Error, B>>>,
        completion_handle: impl FnOnce(CompletionState<Result<ErasedBox, ErrorWith<Error, ErasedBox>>>) -> CompletionHandle,
    ) -> Completion<Result<B, ErrorWith<Error, B>>> {
        let mut executor = self.inner.executor.lock();
        let mut pending = self.inner.pending.lock();
        let pending_entry = pending.vacant_entry();

        let mut state = CompletionState::Pending(None);
        if let Err(mut sqe) = executor.submit([sqe.attach(pending_entry.key())]) {
            let buf = sqe
                .next()
                .expect("submitted one sqe therefore one must be returned on overflow")
                .into_buf()
                .expect("sqe must have been buffer-carrying");
            state = CompletionState::Ready(Err(ErrorWith {
                error: Error::SubmissionQueueOverflow,
                with: buf,
            }));
        }
        let key = pending_entry.key();
        pending_entry.insert(completion_handle(state));

        completion(self.inner.clone(), key)
    }
}

struct SimulatorInner {
    executor: spin::Mutex<Executor<usize>>,
    pending: spin::Mutex<PendingCompletions>,
}

impl Default for SimulatorInner {
    fn default() -> Self {
        Self {
            executor: spin::Mutex::new(Executor::new(<_>::default())),
            pending: <_>::default(),
        }
    }
}

impl SpacetimeIO for SimulatorIO {
    type Fd = fs::File;
    type Error = Error;
    type Completion<T> = Completion<T>;

    fn open_file(&self, path: &str) -> Self::Completion<Result<Self::Fd, Self::Error>> {
        self.submit(Sqe::open(path), Completion::open, CompletionHandle::Open)
    }

    fn create_file(&self, path: &str) -> Self::Completion<Result<Self::Fd, Self::Error>> {
        self.submit(Sqe::create(path), Completion::create, CompletionHandle::Create)
    }

    fn write_all_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>> {
        self.submit_with(
            Sqe::write(fd, ErasedBox::from_aligned(buf), offset),
            Completion::write,
            CompletionHandle::Write,
        )
    }

    fn read_exact_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>> {
        self.submit_with(
            Sqe::read(fd, ErasedBox::from_aligned(buf), offset),
            Completion::read,
            CompletionHandle::Read,
        )
    }

    fn fsync(&self, fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>> {
        self.submit(Sqe::fsync(fd), Completion::fsync, CompletionHandle::Fsync)
    }

    fn fdatasync(&self, fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>> {
        self.submit(Sqe::fdatasync(fd), Completion::fdatasync, CompletionHandle::Fdatasync)
    }

    fn reserve(&self, fd: Self::Fd, total_size: u64) -> Self::Completion<Result<(), Self::Error>> {
        self.submit(
            Sqe::fallocate(fd, total_size),
            Completion::fallocate,
            CompletionHandle::Fallocate,
        )
    }

    fn statx(&self, fd: Self::Fd) -> Self::Completion<Result<Statx, Self::Error>> {
        self.submit(Sqe::stat(fd), Completion::stat, CompletionHandle::Stat)
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_runtime_core::sim::Rng;

    use super::*;

    impl TaskSelector for Rng {
        fn select_tasks(&self, task_count: usize) -> impl IntoIterator<Item = usize> {
            (task_count > 0).then(|| self.index(task_count))
        }
    }

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
                rng: Rng::new(0),
            }
        }

        fn run<T: 'static>(&self, f: impl FnOnce(&SimulatorIO) -> Completion<T>) -> T {
            let fut = self.rt.spawn_local(f(&self.io));
            while self.io.tick(&self.rng, &mut ()) {}
            self.rt.block_on(fut).unwrap()
        }

        fn power_loss(&self) {
            self.io.power_loss();
        }
    }

    #[test]
    fn create_file() {
        let rt = Runtime::new();
        rt.run(|io| io.create_file("/data/test")).unwrap();
    }

    #[derive(Debug)]
    #[repr(C, align(4096))]
    struct Buf<const N: usize>([u8; N]);

    impl<const N: usize> Buf<N> {
        fn clear(&mut self) {
            self.0.fill(0);
        }
    }

    impl<const N: usize> AlignedBytes for Buf<N> {
        fn as_bytes(&self) -> &[u8] {
            &self.0
        }

        fn as_bytes_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }

        fn from_bytes(b: &[u8]) -> Self {
            assert_eq!(b.len(), N);
            let mut buf = [0; N];
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
            .map_err(ErrorWith::into_err)
            .unwrap();
        buf.clear();
        let buf = rt.run(|io| io.read_exact_at(fd, buf, 0)).unwrap();

        assert_eq!(buf.0, [22; 2 * SECTOR_SIZE]);
    }

    #[test]
    fn write_read_at_offset() {
        let rt = Runtime::new();

        let fd = rt.run(|io| io.create_file("/data/test")).unwrap();
        let buf = {
            let mut buf = Buf([0; SECTOR_SIZE]);
            for i in 0usize..2 {
                buf.0.fill((i + 1) as u8 * 2);
                let offset = (i * SECTOR_SIZE) as u64;
                buf = rt
                    .run(|io| io.write_all_at(fd.clone(), buf, offset))
                    .map_err(ErrorWith::into_err)
                    .unwrap();
            }

            buf.clear();
            buf
        };
        let buf = rt.run(|io| io.read_exact_at(fd, buf, SECTOR_SIZE as u64)).unwrap();

        assert_eq!(buf.0, [4; SECTOR_SIZE]);
    }

    #[test]
    fn preallocate() {
        let rt = Runtime::new();

        let fd = rt.run(|io| io.create_file("/data/test")).unwrap();
        rt.run(|io| io.reserve(fd.clone(), 2 * SECTOR_SIZE as u64)).unwrap();

        // Check that reserved space reads as zeroes.
        let buf = rt
            .run(|io| io.read_exact_at(fd.clone(), Buf([1; 2 * SECTOR_SIZE]), 0))
            .unwrap();
        assert_eq!(buf.0, [0; 2 * SECTOR_SIZE]);

        // The length is reported as the preallocated length.
        let stat = rt.run(|io| io.statx(fd.clone())).unwrap();
        assert_eq!(stat.size, 2 * SECTOR_SIZE as u64);

        // Overwriting the second sector works.
        let buf = rt
            .run(|io| io.write_all_at(fd.clone(), Buf([42; SECTOR_SIZE]), SECTOR_SIZE as u64))
            .unwrap();
        let buf = rt
            .run(|io| io.read_exact_at(fd.clone(), buf, SECTOR_SIZE as u64))
            .unwrap();
        assert_eq!(buf.0, [42; SECTOR_SIZE]);
        // The first sector still reads as zeroes.
        let buf = rt.run(|io| io.read_exact_at(fd, buf, 0)).unwrap();
        assert_eq!(buf.0, [0; SECTOR_SIZE]);
    }

    #[test]
    fn open_succeeds_after_create() {
        let rt = Runtime::new();

        matches!(rt.run(|io| io.open_file("/data/test")), Err(Error::FileNotFound { .. }));
        rt.run(|io| io.create_file("/data/test")).unwrap();
        assert!(rt.run(|io| io.open_file("/data/test")).is_ok());
    }

    #[test]
    fn unsynced_data_is_lost_after_power_loss() {
        let rt = Runtime::new();

        let fd = rt.run(|io| io.create_file("/data/test")).unwrap();
        let mut buf = rt
            .run(|io| io.write_all_at(fd.clone(), Buf([1; SECTOR_SIZE]), 0))
            .map_err(ErrorWith::into_err)
            .unwrap();
        buf.clear();

        rt.run(|io| io.fdatasync(fd.clone())).unwrap();

        let mut buf = rt
            .run(|io| io.write_all_at(fd.clone(), Buf([2; SECTOR_SIZE]), SECTOR_SIZE as u64))
            .map_err(ErrorWith::into_err)
            .unwrap();
        buf.clear();

        rt.power_loss();

        let buf = rt.run(|io| io.read_exact_at(fd.clone(), buf, 0)).unwrap();
        assert_eq!(buf.0, [1; SECTOR_SIZE]);
        matches!(
            rt.run(|io| io.read_exact_at(fd.clone(), buf, SECTOR_SIZE as u64))
                .map_err(ErrorWith::into_err),
            Err(Error::UnexpectedEof { .. })
        );
    }
}
