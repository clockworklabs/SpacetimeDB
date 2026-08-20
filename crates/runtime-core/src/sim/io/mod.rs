use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};
use core::{cell::RefCell, num::NonZeroUsize, result::Result};
use futures_channel::oneshot;

use crate::{
    io::{AlignedBytes, ErrorWith, SpacetimeIO},
    sim::Rng,
};

mod fs;
pub mod op;
use op::{Completion, Submission};

pub use crate::io::SECTOR_SIZE;
pub use fs::File;

#[derive(Debug)]
pub enum Error {
    FileNotFound {
        path: Box<str>,
    },
    FileAlreadyExists {
        path: Box<str>,
    },
    ShortWrite {
        expected: usize,
        written: usize,
    },
    UnexpectedEof {
        expected: usize,
        read: usize,
    },
    Fs(fs::Error),
    /// Injected by the I/O driver.
    Cancelled,
}

impl From<fs::Error> for Error {
    fn from(e: fs::Error) -> Self {
        Self::Fs(e)
    }
}

#[derive(Clone, Default)]
pub struct SimulatorIO {
    inner: Rc<RefCell<SimulatorIOInner>>,
}

impl SimulatorIO {
    /// Returns `true` if there are entries in either the submission or
    /// completion queues.
    pub fn pending(&self) -> bool {
        let inner = self.inner.borrow();
        inner.submissions.len() + inner.completions.len() > 0
    }

    /// Number of entries in the submission queue.
    pub fn pending_submissions(&self) -> usize {
        self.inner.borrow().submissions.len()
    }

    /// Number of entries in the completion queue.
    pub fn pending_completions(&self) -> usize {
        self.inner.borrow().completions.len()
    }

    /// Run the submission at the front of the queue (if any), and complete the
    /// completion at the front of the queue (if any).
    pub fn tick(&self) -> bool {
        self.inner.borrow_mut().tick()
    }

    /// Execute `sqe`.
    pub fn execute(&self, sqe: Box<dyn Submission>) {
        self.inner.borrow_mut().execute(sqe);
    }

    /// Remove and return the submission at the front of the queue, if any.
    pub fn next_submission(&self) -> Option<Box<dyn Submission>> {
        self.inner.borrow_mut().next_submission()
    }

    /// Remove and return a random submission, or `None` if the queue is empty.
    pub fn random_submission(&self, rng: &Rng) -> Option<Box<dyn Submission>> {
        self.inner.borrow_mut().random_submission(rng)
    }

    /// Remove and return the completion at the front of the queue, if any.
    pub fn next_completion(&self) -> Option<Box<dyn Completion>> {
        self.inner.borrow_mut().next_completion()
    }

    /// Remove and return a random completion, or `None` if the queue is empty.
    pub fn random_completion(&self, rng: &Rng) -> Option<Box<dyn Completion>> {
        self.inner.borrow_mut().random_completion(rng)
    }

    async fn submit_and_wait<T>(
        &self,
        op: impl FnOnce(oneshot::Sender<T>) -> Box<dyn Submission>,
    ) -> Result<T, oneshot::Canceled> {
        let (tx, rx) = oneshot::channel();
        self.inner.borrow_mut().submit(op(tx));
        rx.await
    }
}

impl SpacetimeIO for SimulatorIO {
    type Fd = fs::File;
    type Error = Error;

    async fn open_file(&self, path: &str) -> Result<Self::Fd, Self::Error> {
        self.submit_and_wait(|tx| op::open_file(path, tx))
            .await
            .expect("`open_file` future cancelled")
    }

    async fn create_file(&self, path: &str, len: u64) -> Result<Self::Fd, Self::Error> {
        self.submit_and_wait(|tx| op::create_file(path, len, tx))
            .await
            .expect("`create_file` future cancelled")
    }

    async fn write_all_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Result<B, ErrorWith<Self::Error, B>> {
        let () = B::ASSERT_VALID_LAYOUT;

        if !offset.is_multiple_of(SECTOR_SIZE as _) {
            self.submit_and_wait(|tx| {
                op::ready(
                    Err(ErrorWith {
                        error: fs::Error::UnalignedOffset.into(),
                        with: buf,
                    }),
                    tx,
                )
            })
            .await
            .expect("`write_all_at` future cancelled")
        } else {
            let (tx, rx) = oneshot::channel();
            for op in op::write_at(fd, buf, offset, tx) {
                self.inner.borrow_mut().submit(op);
            }
            rx.await.expect("`write_all_at` future cancelled")
        }
    }

    async fn read_exact_at<B: AlignedBytes + Send + 'static>(
        &self,
        fd: Self::Fd,
        buf: B,
        offset: u64,
    ) -> Result<B, ErrorWith<Self::Error, B>> {
        let () = B::ASSERT_VALID_LAYOUT;

        if !offset.is_multiple_of(SECTOR_SIZE as _) {
            self.submit_and_wait(|tx| {
                op::ready(
                    Err(ErrorWith {
                        error: fs::Error::UnalignedOffset.into(),
                        with: buf,
                    }),
                    tx,
                )
            })
            .await
            .expect("`read_exact_at` future cancelled")
        } else {
            let (tx, rx) = oneshot::channel();
            for op in op::read_at(fd, buf, offset, tx) {
                self.inner.borrow_mut().submit(op);
            }
            rx.await.expect("`read_exact_at` future cancelled")
        }
    }

    async fn fsync(&self, _fd: Self::Fd) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn fdatasync(&self, _fd: Self::Fd) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn reserve(&self, fd: Self::Fd, additional: u64) -> Result<(), Self::Error> {
        let len = self
            .submit_and_wait(|tx| op::get_len(fd.clone(), tx))
            .await
            .expect("`get_len` future cancelled")?;
        self.submit_and_wait(|tx| op::set_len(fd, len + additional, tx))
            .await
            .expect("`set_len` future cancelled")
    }
}

#[derive(Default)]
struct SimulatorIOInner {
    files: BTreeMap<Box<str>, fs::File>,
    submissions: VecDeque<Box<dyn Submission>>,
    completions: VecDeque<Box<dyn Completion>>,
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

    fn execute(&mut self, sqe: Box<dyn Submission>) {
        if let Some(cqe) = sqe.execute(&mut self.files) {
            self.completions.push_back(cqe);
        }
    }

    fn next_submission(&mut self) -> Option<Box<dyn Submission>> {
        self.submissions.pop_front()
    }

    fn random_submission(&mut self, rng: &Rng) -> Option<Box<dyn Submission>> {
        let len = NonZeroUsize::new(self.submissions.len())?;
        self.submissions.remove(rng.index(len.get()))
    }

    fn next_completion(&mut self) -> Option<Box<dyn Completion>> {
        self.completions.pop_front()
    }

    fn random_completion(&mut self, rng: &Rng) -> Option<Box<dyn Completion>> {
        let len = NonZeroUsize::new(self.completions.len())?;
        self.completions.remove(rng.index(len.get()))
    }

    fn submit(&mut self, op: Box<dyn Submission>) {
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
        let io = rt.io().unwrap();

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
        let io = rt.io().unwrap();

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

    #[test]
    fn nested_handle_block_on_drives_runtime_io() {
        let mut rt = Runtime::with_config(RuntimeConfig::default().enable_io());
        let handle = rt.handle();
        let io = rt.io().unwrap();

        let len = rt.block_on(async move {
            handle
                .block_on(io.create_file("/data/nested", 2 * SECTOR_SIZE as u64))
                .unwrap()
                .len()
        });

        assert_eq!(len, 2 * SECTOR_SIZE as u64);
    }
}
