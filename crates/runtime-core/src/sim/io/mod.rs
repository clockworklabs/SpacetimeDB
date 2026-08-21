use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use core::{
    num::NonZeroUsize,
    pin::Pin,
    result::Result,
    task::{Context, Poll},
};
use futures_channel::oneshot;

use crate::{
    io::{AlignedBytes, ErrorWith, SpacetimeIO},
    sim::Rng,
};

mod fs;
pub mod op;

pub use crate::io::SECTOR_SIZE;
pub use fs::File;

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
}

impl From<fs::Error> for Error {
    fn from(e: fs::Error) -> Self {
        Self::Fs(e)
    }
}

#[derive(Clone, Default)]
pub struct SimulatorIO {
    // TODO: We make `SimulatorIO` `Send + Sync` for now, because
    // [crate::sim::executor::Handle] is just `Arc<Executor>`. This means that a
    // future carrying a handle can't be `spawn`ed, because spawning requires
    // the future to be `Send`.
    //
    // We should fix this at some point, so below can become `Rc<RefCell<_>>`.
    inner: Arc<spin::Mutex<SimulatorIOInner>>,
}

impl SimulatorIO {
    /// Returns `true` if there are entries in either the submission or
    /// completion queues.
    pub fn pending(&self) -> bool {
        let inner = self.inner.lock();
        inner.submissions.len() + inner.completions.len() > 0
    }

    /// Number of entries in the submission queue.
    pub fn pending_submissions(&self) -> usize {
        self.inner.lock().submissions.len()
    }

    /// Number of entries in the completion queue.
    pub fn pending_completions(&self) -> usize {
        self.inner.lock().completions.len()
    }

    /// Run the submission at the front of the queue (if any), and complete the
    /// completion at the front of the queue (if any).
    pub fn tick(&self) -> bool {
        self.inner.lock().tick()
    }

    /// Execute `sqe`.
    pub fn execute(&self, sqe: Box<dyn op::Submission>) {
        self.inner.lock().execute(sqe);
    }

    /// Remove and return the submission at the front of the queue, if any.
    pub fn next_submission(&self) -> Option<Box<dyn op::Submission>> {
        self.inner.lock().next_submission()
    }

    /// Remove and return a random submission, or `None` if the queue is empty.
    pub fn random_submission(&self, rng: &Rng) -> Option<Box<dyn op::Submission>> {
        self.inner.lock().random_submission(rng)
    }

    /// Remove and return the completion at the front of the queue, if any.
    pub fn next_completion(&self) -> Option<Box<dyn op::Completion>> {
        self.inner.lock().next_completion()
    }

    /// Remove and return a random completion, or `None` if the queue is empty.
    pub fn random_completion(&self, rng: &Rng) -> Option<Box<dyn op::Completion>> {
        self.inner.lock().random_completion(rng)
    }

    fn submit<T>(&self, op: impl FnOnce(oneshot::Sender<T>) -> Box<dyn op::Submission>) -> Completion<T> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().submit(op(tx));
        Completion(rx)
    }

    fn submit_all<T, I: Iterator<Item = Box<dyn op::Submission>>>(
        &self,
        ops: impl FnOnce(oneshot::Sender<T>) -> I,
    ) -> Completion<T> {
        let (tx, rx) = oneshot::channel();
        let mut inner = self.inner.lock();
        ops(tx).for_each(|op| inner.submit(op));
        Completion(rx)
    }
}

#[must_use = "completions must be polled to completion"]
pub struct Completion<T>(oneshot::Receiver<T>);

impl<T> Future for Completion<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.as_mut().0).poll(cx).map(Result::unwrap)
    }
}

impl SpacetimeIO for SimulatorIO {
    type Fd = fs::File;
    type Error = Error;
    type Completion<T> = Completion<T>;

    fn open_file(&self, path: &str) -> Self::Completion<Result<Self::Fd, Self::Error>> {
        self.submit(op::open_file(path))
    }

    fn create_file(&self, path: &str, len: u64) -> Self::Completion<Result<Self::Fd, Self::Error>> {
        self.submit(op::create_file(path, len))
    }

    fn write_all_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>> {
        let () = B::ASSERT_VALID_LAYOUT;

        if !offset.is_multiple_of(SECTOR_SIZE as _) {
            self.submit(op::ready(Err(ErrorWith {
                error: fs::Error::UnalignedOffset.into(),
                with: buf,
            })))
        } else {
            self.submit_all(op::write_at(fd, buf, offset))
        }
    }

    fn read_exact_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Self::Completion<Result<B, ErrorWith<Self::Error, B>>> {
        let () = B::ASSERT_VALID_LAYOUT;

        if !offset.is_multiple_of(SECTOR_SIZE as _) {
            self.submit(op::ready(Err(ErrorWith {
                error: fs::Error::UnalignedOffset.into(),
                with: buf,
            })))
        } else {
            self.submit_all(op::read_at(fd, buf, offset))
        }
    }

    fn fsync(&self, _fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>> {
        self.submit(op::ready(Ok(())))
    }

    fn fdatasync(&self, _fd: Self::Fd) -> Self::Completion<Result<(), Self::Error>> {
        self.submit(op::ready(Ok(())))
    }

    fn reserve(&self, fd: Self::Fd, total: u64) -> Self::Completion<Result<(), Self::Error>> {
        assert!(total >= fd.len());
        self.submit(op::set_len(fd, total))
    }

    fn length(&self, fd: Self::Fd) -> Self::Completion<Result<u64, Self::Error>> {
        self.submit(op::get_len(fd))
    }
}

#[derive(Default)]
struct SimulatorIOInner {
    files: BTreeMap<Box<str>, fs::File>,
    submissions: VecDeque<Box<dyn op::Submission>>,
    completions: VecDeque<Box<dyn op::Completion>>,
}

impl SimulatorIOInner {
    fn tick(&mut self) -> bool {
        let mut progress = false;
        if let Some(sqe) = self.submissions.pop_front() {
            if let Some(cqe) = sqe.execute(&mut self.files) {
                self.completions.push_back(cqe);
            }
            progress = true;
        }
        if let Some(cqe) = self.completions.pop_front() {
            cqe.complete();
            progress = true;
        }

        progress
    }

    fn execute(&mut self, sqe: Box<dyn op::Submission>) {
        if let Some(cqe) = sqe.execute(&mut self.files) {
            self.completions.push_back(cqe);
        }
    }

    fn next_submission(&mut self) -> Option<Box<dyn op::Submission>> {
        self.submissions.pop_front()
    }

    fn random_submission(&mut self, rng: &Rng) -> Option<Box<dyn op::Submission>> {
        let len = NonZeroUsize::new(self.submissions.len())?;
        self.submissions.remove(rng.index(len.get()))
    }

    fn next_completion(&mut self) -> Option<Box<dyn op::Completion>> {
        self.completions.pop_front()
    }

    fn random_completion(&mut self, rng: &Rng) -> Option<Box<dyn op::Completion>> {
        let len = NonZeroUsize::new(self.completions.len())?;
        self.completions.remove(rng.index(len.get()))
    }

    fn submit(&mut self, op: Box<dyn op::Submission>) {
        self.submissions.push_back(op);
    }
}

#[cfg(test)]
mod tests {
    use crate::sim::{Runtime, RuntimeConfig};

    use super::*;

    #[test]
    fn create_file() {
        let mut rt = Runtime::with_config(RuntimeConfig::default().enable_io());
        let io = rt.io().cloned().unwrap();

        let fd = rt
            .block_on(io.create_file("/data/test", 2 * SECTOR_SIZE as u64))
            .unwrap();
        assert_eq!(fd.len(), 2 * SECTOR_SIZE as u64);
    }

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
        let mut rt = Runtime::with_config(RuntimeConfig::default().enable_io());
        let io = rt.io().cloned().unwrap();

        let fd = rt
            .block_on(io.create_file("/data/test", 2 * SECTOR_SIZE as u64))
            .unwrap();
        let mut buf = rt
            .block_on(io.write_all_at(fd.clone(), Buf([22; 2 * SECTOR_SIZE]), 0))
            .map_err(|ErrorWith { error, .. }| error)
            .unwrap();
        buf.clear();
        let buf = rt
            .block_on(io.read_exact_at(fd, buf, 0))
            .map_err(|ErrorWith { error, .. }| error)
            .unwrap();

        assert!(buf.0.iter().all(|&b| b == 22));
    }
}
