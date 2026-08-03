use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};
use core::{
    cell::RefCell,
    future::{poll_fn, Future},
    pin::Pin,
    result::Result,
    task::Poll,
};
use futures_channel::oneshot;

use crate::io::{AlignedBytes, ErrorWith, SpacetimeIO};

mod fs;
mod op;

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

#[derive(Default)]
pub struct SimulatorIO {
    inner: Rc<RefCell<SimulatorIOInner>>,
}

impl SimulatorIO {
    pub fn tick(&self) {
        self.inner.borrow_mut().tick();
    }

    fn submit<T>(&self, op: impl FnOnce(oneshot::Sender<T>) -> Box<dyn Submission>) -> oneshot::Receiver<T> {
        let (tx, rx) = oneshot::channel();
        self.inner.borrow_mut().submit(op(tx));
        rx
    }

    // TODO: The sim runtime should be advancing I/O. Until it does, `tick()`
    // whenever a result future is polled and returns pending.
    async fn wait_for<T>(&self, mut rx: oneshot::Receiver<T>) -> Result<T, oneshot::Canceled> {
        poll_fn(|cx| match Pin::new(&mut rx).poll(cx) {
            Poll::Ready(result) => Poll::Ready(result),
            Poll::Pending => {
                self.tick();
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await
    }

    async fn submit_and_wait<T>(
        &self,
        op: impl FnOnce(oneshot::Sender<T>) -> Box<dyn Submission>,
    ) -> Result<T, oneshot::Canceled> {
        let rx = self.submit(op);
        self.wait_for(rx).await
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

    async fn write_all_at<B: AlignedBytes + 'static>(
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
            self.wait_for(rx).await.expect("`write_all_at` future cancelled")
        }
    }

    async fn read_exact_at<B: AlignedBytes + 'static>(
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
            self.wait_for(rx).await.expect("`read_exact_at` future cancelled")
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
    fn tick(&mut self) {
        if let Some(sqe) = self.submissions.pop_front() {
            sqe.execute(&mut self.files, &mut self.completions);
        }
        if let Some(cqe) = self.completions.pop_front() {
            cqe.complete();
        }
    }

    fn submit(&mut self, op: Box<dyn Submission>) {
        self.submissions.push_back(op);
    }
}

trait Submission {
    fn execute(
        self: Box<Self>,
        files: &mut BTreeMap<Box<str>, fs::File>,
        completions: &mut VecDeque<Box<dyn Completion>>,
    );
}

trait Completion {
    fn complete(self: Box<Self>);
}

#[cfg(test)]
mod tests {
    use crate::sim::Runtime;

    use super::*;

    #[test]
    fn create_file() {
        let mut rt = Runtime::new(1);
        let io = SimulatorIO::default();

        let fd = rt
            .block_on(io.create_file("/data/test", 2 * SECTOR_SIZE as u64))
            .unwrap();
        assert_eq!(fd.len(), 2 * SECTOR_SIZE as u64);
    }

    #[repr(C, align(4096))]
    struct Buf([u8; 2 * SECTOR_SIZE]);

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
        let mut rt = Runtime::new(1);
        let io = SimulatorIO::default();

        let fd = rt
            .block_on(io.create_file("/data/test", 2 * SECTOR_SIZE as u64))
            .unwrap();
        let mut buf = rt
            .block_on(io.write_all_at(fd.clone(), Buf([22; 2 * SECTOR_SIZE]), 0))
            .map_err(|ErrorWith { error, .. }| error)
            .unwrap();
        buf.0.fill(0);
        let buf = rt
            .block_on(io.read_exact_at(fd, buf, 0))
            .map_err(|ErrorWith { error, .. }| error)
            .unwrap();

        assert!(buf.0.iter().all(|&b| b == 22));
    }
}
