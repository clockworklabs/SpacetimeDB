use core::{any::Any, iter::Scan, ops::Range};

use alloc::{
    boxed::Box,
    collections::{btree_map, BTreeMap},
    sync::Arc,
};
use futures_channel::oneshot;

use super::{fs, Error};
use crate::io::{AlignedBytes, ErrorWith, SECTOR_SIZE};

/// An operation that can be submitted to the [super::SimulatorIO] driver.
pub trait Submission: Send + Any {
    /// Run the operations with mutable access to the currently registered
    /// [fs::File]s.
    ///
    /// If the operation is done, a [Completion] is returned in a `Some`.
    /// `None` may be returned if:
    ///
    /// - The submission is a sub-operation, such as [WritePage] or [ReadPage].
    /// - The submission is a [Noop].
    ///
    fn execute(self: Box<Self>, files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>>;

    /// Cancel the operation instead of executing it.
    ///
    /// This will generate a [Completion] with the result [Error::Cancelled],
    /// unless:
    ///
    /// - The submission is a sub-operation, such as [WritePage] or [ReadPage].
    /// - The submission is a [Noop].
    ///
    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>>;
}

/// An object containing the result of executing a [Submission], as well as a
/// handle to resolve a future waiting on the outcome of the operation.
pub trait Completion: Send {
    fn success(&self) -> bool;
    /// Resolve the future waiting on the outcome of the operation.
    fn complete(self: Box<Self>);
}

/// A channel to resolve a future waiting on the outcome of a submitted
/// operation.
pub type OnComplete<T> = oneshot::Sender<T>;

pub type ScanState<B> = (fs::File, usize, Arc<spin::Mutex<PagedOpState<B>>>);
pub type PageWrites<B> = Scan<Range<usize>, ScanState<B>, fn(&mut ScanState<B>, usize) -> Option<Box<dyn Submission>>>;
pub type PageReads<B> = Scan<Range<usize>, ScanState<B>, fn(&mut ScanState<B>, usize) -> Option<Box<dyn Submission>>>;

pub type WriteAtResult<B> = Result<B, ErrorWith<Error, B>>;

/// Write the contents of `buf` to `fd` at `offset`.
///
/// This operation is split into multiple writes to individual pages. The
/// `on_complete` future resolves only after all page writes completed.
///
/// Ownership of `buf` is transferred back when the operation completes.
pub fn write_at<B: AlignedBytes + Send + 'static>(
    fd: fs::File,
    buf: B,
    offset: u64,
) -> impl FnOnce(OnComplete<WriteAtResult<B>>) -> PageWrites<B> {
    move |on_complete| {
        let first_page = (offset / SECTOR_SIZE as u64) as usize;
        let page_count = buf.as_bytes().len() / SECTOR_SIZE;

        let state = Arc::new(spin::Mutex::new(PagedOpState {
            buf: Some(buf),
            on_complete: Some(on_complete),
            remaining: page_count,
            first_error: None,
        }));

        (0..page_count).scan((fd, first_page, state), |(fd, first_page, state), buf_page| {
            let op = WritePage {
                fd: fd.clone(),
                file_page: *first_page + buf_page,
                buf_page,
                state: state.clone(),
            };

            Some(Box::new(op))
        })
    }
}

pub type ReadAtResult<B> = Result<B, ErrorWith<Error, B>>;

/// Fill `buf` by reading from `fd` at `offset`.
///
/// This operation is split into multple reads from the individual pages needed
/// to fill `buf`. The `on_complete` future resolves only after all page reads
/// completed.
///
/// Ownership of `buf` is transferred back when the operation completes.
pub fn read_at<B: AlignedBytes + Send + 'static>(
    fd: fs::File,
    buf: B,
    offset: u64,
) -> impl FnOnce(OnComplete<ReadAtResult<B>>) -> PageReads<B> {
    move |on_complete| {
        let first_page = (offset / SECTOR_SIZE as u64) as usize;
        let page_count = buf.as_bytes().len() / SECTOR_SIZE;

        let state = Arc::new(spin::Mutex::new(PagedOpState {
            buf: Some(buf),
            on_complete: Some(on_complete),
            remaining: page_count,
            first_error: None,
        }));

        (0..page_count).scan((fd, first_page, state), |(fd, first_page, state), buf_page| {
            let op = ReadPage {
                fd: fd.clone(),
                file_page: *first_page + buf_page,
                buf_page,
                state: state.clone(),
            };

            Some(Box::new(op))
        })
    }
}

/// Open file at `path`.
pub fn open_file(path: &str) -> impl FnOnce(OnComplete<Result<fs::File, Error>>) -> Box<dyn Submission> {
    move |on_complete| {
        Box::new(OpenFile {
            path: path.into(),
            on_complete,
        })
    }
}

/// Create a new file at `path` and allocate `len` space for it.
pub fn create_file(path: &str, len: u64) -> impl FnOnce(OnComplete<Result<fs::File, Error>>) -> Box<dyn Submission> {
    move |on_complete| {
        Box::new(CreateFile {
            path: path.into(),
            len,
            on_complete,
        })
    }
}

/// Get the length of the file `fd`.
pub fn get_len(fd: fs::File) -> impl FnOnce(OnComplete<Result<u64, Error>>) -> Box<dyn Submission> {
    move |on_complete| Box::new(GetLen { fd, on_complete })
}

/// Set the length of the file `fd`.
pub fn set_len(fd: fs::File, len: u64) -> impl FnOnce(OnComplete<Result<(), Error>>) -> Box<dyn Submission> {
    move |on_complete| Box::new(SetLen { fd, len, on_complete })
}

struct GenericCompletion<T> {
    success: bool,
    result: T,
    on_complete: OnComplete<T>,
}

fn completion<T: Send + 'static>(success: bool, result: T, on_complete: OnComplete<T>) -> Box<dyn Completion> {
    Box::new(GenericCompletion {
        success,
        result,
        on_complete,
    })
}

impl<T: Send> Completion for GenericCompletion<T> {
    fn success(&self) -> bool {
        self.success
    }

    fn complete(self: Box<Self>) {
        let Self {
            result, on_complete, ..
        } = *self;
        let _ = on_complete.send(result);
    }
}

/// [Submission] created by [noop].
pub(crate) struct Noop;

impl Submission for Noop {
    fn execute(self: Box<Self>, _files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        None
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        None
    }
}

/// An operation that does nothing.
///
/// Note that no completion is associated with a noop, but the submission still
/// occupies a slot in the submission queue.
pub fn noop() -> Box<dyn Submission> {
    Box::new(Noop)
}

/// [Submission] created by [ready].
pub(crate) struct Ready(Box<dyn Completion>);

impl Submission for Ready {
    fn execute(self: Box<Self>, _files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self(completion) = *self;
        Some(completion)
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self(completion) = *self;
        Some(completion)
    }
}

/// An operation that is already complete with `result`.
pub fn ready<T: Send + 'static>(result: T) -> impl FnOnce(OnComplete<T>) -> Box<dyn Submission> {
    move |on_complete| Box::new(Ready(completion(true, result, on_complete)))
}

/// [Submission] created by [link].
pub(crate) struct SoftLink {
    a: Box<dyn Submission>,
    b: Box<dyn Submission>,
}

impl Submission for SoftLink {
    fn execute(self: Box<Self>, files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self { a, b } = *self;
        let result_a = a.execute(files);
        let result_b = if result_a.as_ref().is_none_or(|result| result.success()) {
            b.execute(files)
        } else {
            b.cancel()
        };

        Some(Box::new(LinkedCompletion {
            a: result_a,
            b: result_b,
        }))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self { a, b } = *self;
        Some(Box::new(LinkedCompletion {
            a: a.cancel(),
            b: b.cancel(),
        }))
    }
}

/// Link `a` and `b`, such that `b` gets executed after `a`.
///
/// If `a` fails (i.e. its [Completion::success] returns `false`), `b` is
/// cancelled.
///
/// Corresponds to io-uring's `IOSQE_IO_LINK` flag. To emulate
/// `IOSQE_IO_HARDLINK`, see [hard_link].
pub fn link(a: Box<dyn Submission>, b: Box<dyn Submission>) -> Box<dyn Submission> {
    Box::new(SoftLink { a, b })
}

/// [Submission] created by [hard_link].
pub(crate) struct HardLink {
    a: Box<dyn Submission>,
    b: Box<dyn Submission>,
}

impl Submission for HardLink {
    fn execute(self: Box<Self>, files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self { a, b } = *self;
        Some(Box::new(LinkedCompletion {
            a: a.execute(files),
            b: b.execute(files),
        }))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self { a, b } = *self;
        Some(Box::new(LinkedCompletion {
            a: a.cancel(),
            b: b.cancel(),
        }))
    }
}

/// Link `a` and `b`, such that `b` gets executed after `a`.
///
/// Unlike [link], this executes both submissions regardless of the result. It
/// just enforces the ordering constraint that `b` will never execute before
/// `a`.
///
/// Corresponds to io-uring's `IOSQE_IO_HARDLINK` flag. To emulate
/// `IOSQE_IO_LINK`, see [link].
pub fn hard_link(a: Box<dyn Submission>, b: Box<dyn Submission>) -> Box<dyn Submission> {
    Box::new(HardLink { a, b })
}

struct LinkedCompletion {
    a: Option<Box<dyn Completion>>,
    b: Option<Box<dyn Completion>>,
}

impl Completion for LinkedCompletion {
    fn success(&self) -> bool {
        self.a.as_ref().is_none_or(|result| result.success()) && self.b.as_ref().is_none_or(|result| result.success())
    }

    fn complete(self: Box<Self>) {
        let Self { a, b } = *self;
        if let Some(a) = a {
            a.complete();
        }
        if let Some(b) = b {
            b.complete();
        }
    }
}

/// [Submission] created by [open_file].
pub(crate) struct OpenFile {
    path: Box<str>,
    on_complete: OnComplete<Result<fs::File, Error>>,
}

impl Submission for OpenFile {
    fn execute(self: Box<Self>, files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self { path, on_complete } = *self;
        let result = files.get(&path).cloned().ok_or(Error::FileNotFound { path });
        Some(completion(result.is_ok(), result, on_complete))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self { path: _, on_complete } = *self;
        Some(completion(false, Err(Error::Cancelled), on_complete))
    }
}

/// [Submission] created by [create_file].
pub(crate) struct CreateFile {
    path: Box<str>,
    len: u64,
    on_complete: OnComplete<Result<fs::File, Error>>,
}

impl Submission for CreateFile {
    fn execute(self: Box<Self>, files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self { path, len, on_complete } = *self;
        let result = (|| {
            let file = match files.entry(path.clone()) {
                btree_map::Entry::Vacant(entry) => Ok(entry.insert(fs::File::new()).clone()),
                btree_map::Entry::Occupied(_) => Err(Error::FileAlreadyExists { path }),
            }?;
            file.set_len(len)?;
            Ok(file)
        })();
        Some(completion(result.is_ok(), result, on_complete))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self {
            path: _,
            len: _,
            on_complete,
        } = *self;
        Some(completion(false, Err(Error::Cancelled), on_complete))
    }
}

/// [Submission] created by [get_len].
pub(crate) struct GetLen {
    fd: fs::File,
    on_complete: OnComplete<Result<u64, Error>>,
}

impl Submission for GetLen {
    fn execute(self: Box<Self>, _files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self { fd, on_complete } = *self;
        let result = Ok(fd.len());
        Some(completion(true, result, on_complete))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self { fd: _, on_complete } = *self;
        Some(completion(false, Err(Error::Cancelled), on_complete))
    }
}

/// [Submission] created by [set_len].
pub(crate) struct SetLen {
    fd: fs::File,
    len: u64,
    on_complete: OnComplete<Result<(), Error>>,
}

impl Submission for SetLen {
    fn execute(self: Box<Self>, _files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self { fd, len, on_complete } = *self;
        let result = fd.set_len(len).map_err(Error::from);
        Some(completion(result.is_ok(), result, on_complete))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self {
            fd: _,
            len: _,
            on_complete,
        } = *self;
        Some(completion(false, Err(Error::Cancelled), on_complete))
    }
}

pub struct PagedOpState<B> {
    buf: Option<B>,
    on_complete: Option<OnComplete<Result<B, ErrorWith<Error, B>>>>,
    remaining: usize,
    first_error: Option<Error>,
}

fn complete_page_op<B: Send + 'static>(
    state: &Arc<spin::Mutex<PagedOpState<B>>>,
    result: Result<(), Error>,
) -> Option<Box<PageOpCompletion<B>>> {
    let complete = {
        let mut state = state.lock();
        if let Err(e) = result
            && state.first_error.is_none()
        {
            state.first_error.replace(e);
        }
        assert!(state.remaining > 0);
        state.remaining -= 1;

        state.remaining == 0
    };

    complete.then(|| Box::new(PageOpCompletion { state: state.clone() }))
}

struct PageOpCompletion<B> {
    state: Arc<spin::Mutex<PagedOpState<B>>>,
}

impl<B: Send + 'static> Completion for PageOpCompletion<B> {
    fn success(&self) -> bool {
        self.state.lock().first_error.is_none()
    }

    fn complete(self: Box<Self>) {
        let (on_complete, result) = {
            let mut state = self.state.lock();

            assert_eq!(state.remaining, 0);

            let buf = state.buf.take().expect("write completed more than once");
            let on_complete = state.on_complete.take().expect("write completed more than once");

            let result = match state.first_error.take() {
                None => Ok(buf),
                Some(error) => Err(ErrorWith { error, with: buf }),
            };

            (on_complete, result)
        };

        let _ = on_complete.send(result);
    }
}

pub(crate) struct WritePage<B> {
    fd: fs::File,
    file_page: usize,
    buf_page: usize,
    state: Arc<spin::Mutex<PagedOpState<B>>>,
}

impl<B: AlignedBytes + Send + 'static> Submission for WritePage<B> {
    fn execute(self: Box<Self>, _files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self {
            fd,
            file_page,
            buf_page,
            state,
        } = *self;

        let result = {
            let state_ref = state.lock();
            let buf = state_ref.buf.as_ref().expect("buffer went away");

            let start = buf_page * SECTOR_SIZE;
            let end = start + SECTOR_SIZE;
            fd.write_page(&buf.as_bytes()[start..end], file_page as _)
        };
        complete_page_op(&state, result.map_err(Into::into)).map(|c| c as Box<dyn Completion>)
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self {
            fd: _,
            file_page: _,
            buf_page: _,
            state,
        } = *self;
        complete_page_op(&state, Err(Error::Cancelled)).map(|c| c as Box<dyn Completion>)
    }
}

pub(crate) struct ReadPage<B> {
    fd: fs::File,
    file_page: usize,
    buf_page: usize,
    state: Arc<spin::Mutex<PagedOpState<B>>>,
}

impl<B: AlignedBytes + Send + 'static> Submission for ReadPage<B> {
    fn execute(self: Box<Self>, _files: &mut BTreeMap<Box<str>, fs::File>) -> Option<Box<dyn Completion>> {
        let Self {
            fd,
            file_page,
            buf_page,
            state,
        } = *self;

        let result = {
            let mut state_ref = state.lock();
            let buf = state_ref.buf.as_mut().expect("buffer went away");

            let start = buf_page * SECTOR_SIZE;
            let end = start + SECTOR_SIZE;
            fd.read_page(&mut buf.as_bytes_mut()[start..end], file_page as _)
        };
        complete_page_op(&state, result.map_err(Into::into)).map(|c| c as Box<dyn Completion>)
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self {
            fd: _,
            file_page: _,
            buf_page: _,
            state,
        } = *self;
        complete_page_op(&state, Err(Error::Cancelled)).map(|c| c as Box<dyn Completion>)
    }
}

#[cfg(test)]
mod tests {
    use core::any::Any;

    use super::*;

    #[test]
    fn downcast() {
        let sqe: Box<dyn Any> = noop();
        sqe.downcast::<Noop>().unwrap();
    }
}
