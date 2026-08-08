use core::any::Any;

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
    /// Resolve the future waiting on the outcome of the operation.
    fn complete(self: Box<Self>);
}

/// A channel to resolve a future waiting on the outcome of a submitted
/// operation.
pub type OnComplete<T> = oneshot::Sender<T>;

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
    on_complete: OnComplete<WriteAtResult<B>>,
) -> impl Iterator<Item = Box<dyn Submission>> {
    let first_page = (offset / SECTOR_SIZE as u64) as usize;
    let page_count = buf.as_bytes().len() / SECTOR_SIZE;

    let state = Arc::new(spin::Mutex::new(PagedOpState {
        buf: Some(buf),
        on_complete: Some(on_complete),
        remaining: page_count,
        first_error: None,
    }));

    (0..page_count).map(move |buf_page| {
        let op = WritePage {
            fd: fd.clone(),
            file_page: first_page + buf_page,
            buf_page,
            state: state.clone(),
        };

        Box::new(op) as Box<dyn Submission>
    })
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
    on_complete: OnComplete<ReadAtResult<B>>,
) -> impl Iterator<Item = Box<dyn Submission>> {
    let first_page = (offset / SECTOR_SIZE as u64) as usize;
    let page_count = buf.as_bytes().len() / SECTOR_SIZE;

    let state = Arc::new(spin::Mutex::new(PagedOpState {
        buf: Some(buf),
        on_complete: Some(on_complete),
        remaining: page_count,
        first_error: None,
    }));

    (0..page_count).map(move |buf_page| {
        let op = ReadPage {
            fd: fd.clone(),
            file_page: first_page + buf_page,
            buf_page,
            state: state.clone(),
        };

        Box::new(op) as Box<dyn Submission>
    })
}

/// Open file at `path`.
pub fn open_file(path: &str, on_complete: OnComplete<Result<fs::File, Error>>) -> Box<dyn Submission> {
    Box::new(OpenFile {
        path: path.into(),
        on_complete,
    })
}

/// Create a new file at `path` and allocate `len` space for it.
pub fn create_file(path: &str, len: u64, on_complete: OnComplete<Result<fs::File, Error>>) -> Box<dyn Submission> {
    Box::new(CreateFile {
        path: path.into(),
        len,
        on_complete,
    })
}

/// Get the length of the file `fd`.
pub fn get_len(fd: fs::File, on_complete: OnComplete<Result<u64, Error>>) -> Box<dyn Submission> {
    Box::new(GetLen { fd, on_complete })
}

/// Set the length of the file `fd`.
pub fn set_len(fd: fs::File, len: u64, on_complete: OnComplete<Result<(), Error>>) -> Box<dyn Submission> {
    Box::new(SetLen { fd, len, on_complete })
}

struct GenericCompletion<T> {
    result: T,
    on_complete: OnComplete<T>,
}

fn completion<T: Send + 'static>(result: T, on_complete: OnComplete<T>) -> Box<dyn Completion> {
    Box::new(GenericCompletion { result, on_complete })
}

impl<T: Send> Completion for GenericCompletion<T> {
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
pub fn ready<T: Send + 'static>(result: T, on_complete: OnComplete<T>) -> Box<dyn Submission> {
    Box::new(Ready(completion(result, on_complete)))
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
        Some(completion(result, on_complete))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self { path: _, on_complete } = *self;
        Some(completion(Err(Error::Cancelled), on_complete))
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
        Some(completion(result, on_complete))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self {
            path: _,
            len: _,
            on_complete,
        } = *self;
        Some(completion(Err(Error::Cancelled), on_complete))
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
        Some(completion(result, on_complete))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self { fd: _, on_complete } = *self;
        Some(completion(Err(Error::Cancelled), on_complete))
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
        Some(completion(result, on_complete))
    }

    fn cancel(self: Box<Self>) -> Option<Box<dyn Completion>> {
        let Self {
            fd: _,
            len: _,
            on_complete,
        } = *self;
        Some(completion(Err(Error::Cancelled), on_complete))
    }
}

struct PagedOpState<B> {
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
