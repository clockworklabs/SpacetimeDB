use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    vec::Vec,
};
use core::{ops::RangeBounds, result::Result};
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
    FileNotFound { path: Box<str> },
    FileAlreadyExists { path: Box<str> },
    ShortWrite { expected: usize, written: usize },
    UnexpectedEof { expected: usize, read: usize },
    Fs(fs::Error),
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
    /// Run the submission at the front of the queue (if any), and complete the
    /// completion at the front of the queue (if any).
    pub fn tick(&self) -> bool {
        self.inner.lock().tick()
    }

    /// Execute `sqe`.
    pub fn execute(&self, sqe: Box<dyn Submission>) {
        self.inner.lock().execute(sqe);
    }

    /// Remove and return the submission at the fron of the queue, if any.
    pub fn next_submission(&self) -> Option<Box<dyn Submission>> {
        self.inner.lock().next()
    }

    /// Remove and return a random submission, or `None` if the queue is empty.
    pub fn random_submission(&self, rng: &Rng) -> Option<Box<dyn Submission>> {
        self.inner.lock().next_random(rng)
    }

    /// Remove `range` from the completion queue.
    pub fn completions(&self, range: impl RangeBounds<usize>) -> impl Iterator<Item = Box<dyn Completion>> {
        self.inner
            .lock()
            .drain_completions(range)
            .collect::<Vec<_>>()
            .into_iter()
    }

    async fn submit_and_wait<T>(
        &self,
        op: impl FnOnce(oneshot::Sender<T>) -> Box<dyn Submission>,
    ) -> Result<T, oneshot::Canceled> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().submit(op(tx));
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
                self.inner.lock().submit(op);
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
                self.inner.lock().submit(op);
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
    // TODO: Allow runtime to inject faults via:
    //
    // - pick random entries from the submission queue
    // - drop queue entries
    // - delay `execute` (somehow)
    // - delay `complete`
    // - make a submission fail without performing its effect
    // - execute an arbitrary number of (random) SQEs
    // - complete an arbitrary number of CQEs

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

    fn next(&mut self) -> Option<Box<dyn Submission>> {
        self.submissions.pop_front()
    }

    fn next_random(&mut self, rng: &Rng) -> Option<Box<dyn Submission>> {
        let i = rng.next_u64() % self.submissions.len() as u64;
        self.submissions.remove(i as usize)
    }

    fn drain_completions(&mut self, range: impl RangeBounds<usize>) -> impl Iterator<Item = Box<dyn Completion>> {
        self.completions.drain(range)
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
        let mut rt = Runtime::with_config(RuntimeConfig {
            enable_io: true,
            ..<_>::default()
        });
        let io = rt.io().clone().unwrap();

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
        let mut rt = Runtime::with_config(RuntimeConfig {
            enable_io: true,
            ..<_>::default()
        });
        let io = rt.io().clone().unwrap();

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
