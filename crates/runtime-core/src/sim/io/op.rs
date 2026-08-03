use alloc::{
    boxed::Box,
    collections::{btree_map, BTreeMap, VecDeque},
    rc::Rc,
};
use core::cell::RefCell;
use futures_channel::oneshot;

use super::{fs, Completion, Error, Submission};
use crate::io::{AlignedBytes, ErrorWith, SECTOR_SIZE};

pub type WriteAtResult<B> = Result<B, ErrorWith<Error, B>>;
pub type ReadAtResult<B> = Result<B, ErrorWith<Error, B>>;

pub fn write_at<B: AlignedBytes + 'static>(
    fd: fs::File,
    buf: B,
    offset: u64,
    notify: oneshot::Sender<WriteAtResult<B>>,
) -> impl Iterator<Item = Box<dyn Submission>> {
    let first_page = (offset / SECTOR_SIZE as u64) as usize;
    let page_count = buf.as_bytes().len() / SECTOR_SIZE;

    let state = Rc::new(RefCell::new(PagedOpState {
        buf: Some(buf),
        notify: Some(notify),
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

pub fn read_at<B: AlignedBytes + 'static>(
    fd: fs::File,
    buf: B,
    offset: u64,
    notify: oneshot::Sender<ReadAtResult<B>>,
) -> impl Iterator<Item = Box<dyn Submission>> {
    let first_page = (offset / SECTOR_SIZE as u64) as usize;
    let page_count = buf.as_bytes().len() / SECTOR_SIZE;

    let state = Rc::new(RefCell::new(PagedOpState {
        buf: Some(buf),
        notify: Some(notify),
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

pub fn open_file(path: &str, notify: oneshot::Sender<Result<fs::File, Error>>) -> Box<dyn Submission> {
    Box::new(OpenFile {
        path: path.into(),
        notify,
    })
}

pub fn create_file(path: &str, len: u64, notify: oneshot::Sender<Result<fs::File, Error>>) -> Box<dyn Submission> {
    Box::new(CreateFile {
        path: path.into(),
        len,
        notify,
    })
}

pub fn get_len(fd: fs::File, notify: oneshot::Sender<Result<u64, Error>>) -> Box<dyn Submission> {
    Box::new(GetLen { fd, notify })
}

pub fn set_len(fd: fs::File, len: u64, notify: oneshot::Sender<Result<(), Error>>) -> Box<dyn Submission> {
    Box::new(SetLen { fd, len, notify })
}

struct GenericCompletion<T> {
    result: T,
    notify: oneshot::Sender<T>,
}

fn completion<T: 'static>(result: T, notify: oneshot::Sender<T>) -> Box<dyn Completion> {
    Box::new(GenericCompletion { result, notify })
}

impl<T> Completion for GenericCompletion<T> {
    fn complete(self: Box<Self>) {
        let Self { result, notify } = *self;
        let _ = notify.send(result);
    }
}

struct Ready(Box<dyn Completion>);

impl Submission for Ready {
    fn execute(
        self: Box<Self>,
        _files: &mut BTreeMap<Box<str>, fs::File>,
        completions: &mut VecDeque<Box<dyn Completion>>,
    ) {
        let Self(completion) = *self;
        completions.push_back(completion);
    }
}

pub fn ready<T: 'static>(result: T, notify: oneshot::Sender<T>) -> Box<dyn Submission> {
    Box::new(Ready(completion(result, notify)))
}

struct OpenFile {
    path: Box<str>,
    notify: oneshot::Sender<Result<fs::File, Error>>,
}

impl Submission for OpenFile {
    fn execute(
        self: Box<Self>,
        files: &mut BTreeMap<Box<str>, fs::File>,
        completions: &mut VecDeque<Box<dyn Completion>>,
    ) {
        let Self { path, notify } = *self;
        let result = files.get(&path).cloned().ok_or(Error::FileNotFound { path });
        completions.push_back(completion(result, notify));
    }
}

struct CreateFile {
    path: Box<str>,
    len: u64,
    notify: oneshot::Sender<Result<fs::File, Error>>,
}

impl Submission for CreateFile {
    fn execute(
        self: Box<Self>,
        files: &mut BTreeMap<Box<str>, fs::File>,
        completions: &mut VecDeque<Box<dyn Completion>>,
    ) {
        let Self { path, len, notify } = *self;
        let result = (|| {
            let file = match files.entry(path.clone()) {
                btree_map::Entry::Vacant(entry) => Ok(entry.insert(fs::File::new()).clone()),
                btree_map::Entry::Occupied(_) => Err(Error::FileAlreadyExists { path }),
            }?;
            file.set_len(len)?;
            Ok(file)
        })();
        completions.push_back(completion(result, notify));
    }
}

struct GetLen {
    fd: fs::File,
    notify: oneshot::Sender<Result<u64, Error>>,
}

impl Submission for GetLen {
    fn execute(
        self: Box<Self>,
        _files: &mut BTreeMap<Box<str>, fs::File>,
        completions: &mut VecDeque<Box<dyn Completion>>,
    ) {
        let Self { fd, notify } = *self;
        let result = Ok(fd.len());
        completions.push_back(completion(result, notify));
    }
}

struct SetLen {
    fd: fs::File,
    len: u64,
    notify: oneshot::Sender<Result<(), Error>>,
}

impl Submission for SetLen {
    fn execute(
        self: Box<Self>,
        _files: &mut BTreeMap<Box<str>, fs::File>,
        completions: &mut VecDeque<Box<dyn Completion>>,
    ) {
        let Self { fd, len, notify } = *self;
        let result = fd.set_len(len).map_err(Error::from);
        completions.push_back(completion(result, notify));
    }
}

struct PagedOpState<B> {
    buf: Option<B>,
    notify: Option<oneshot::Sender<Result<B, ErrorWith<Error, B>>>>,
    remaining: usize,
    first_error: Option<Error>,
}

fn complete_page_op<B: 'static>(
    state: &Rc<RefCell<PagedOpState<B>>>,
    result: Result<(), fs::Error>,
    completions: &mut VecDeque<Box<dyn Completion>>,
) {
    let complete = {
        let mut state = state.borrow_mut();
        if let Err(e) = result
            && state.first_error.is_none()
        {
            state.first_error.replace(e.into());
        }
        assert!(state.remaining > 0);
        state.remaining -= 1;

        state.remaining == 0
    };

    if complete {
        completions.push_back(Box::new(WriteCompletion { state: state.clone() }));
    }
}

struct WriteCompletion<B> {
    state: Rc<RefCell<PagedOpState<B>>>,
}

impl<B: 'static> Completion for WriteCompletion<B> {
    fn complete(self: Box<Self>) {
        let (notify, result) = {
            let mut state = self.state.borrow_mut();

            assert_eq!(state.remaining, 0);

            let buf = state.buf.take().expect("write completed more than once");
            let notify = state.notify.take().expect("write completed more than once");

            let result = match state.first_error.take() {
                None => Ok(buf),
                Some(error) => Err(ErrorWith { error, with: buf }),
            };

            (notify, result)
        };

        let _ = notify.send(result);
    }
}

struct WritePage<B> {
    fd: fs::File,
    file_page: usize,
    buf_page: usize,
    state: Rc<RefCell<PagedOpState<B>>>,
}

impl<B: AlignedBytes + 'static> Submission for WritePage<B> {
    fn execute(
        self: Box<Self>,
        _files: &mut BTreeMap<Box<str>, fs::File>,
        completions: &mut VecDeque<Box<dyn Completion>>,
    ) {
        let Self {
            fd,
            file_page,
            buf_page,
            state,
        } = *self;

        let result = {
            let state_ref = state.borrow();
            let buf = state_ref.buf.as_ref().expect("buffer went away");

            let start = buf_page * SECTOR_SIZE;
            let end = start + SECTOR_SIZE;
            fd.write_page(&buf.as_bytes()[start..end], file_page as _)
        };
        complete_page_op(&state, result, completions);
    }
}

struct ReadPage<B> {
    fd: fs::File,
    file_page: usize,
    buf_page: usize,
    state: Rc<RefCell<PagedOpState<B>>>,
}

impl<B: AlignedBytes + 'static> Submission for ReadPage<B> {
    fn execute(
        self: Box<Self>,
        _files: &mut BTreeMap<Box<str>, fs::File>,
        completions: &mut VecDeque<Box<dyn Completion>>,
    ) {
        let Self {
            fd,
            file_page,
            buf_page,
            state,
        } = *self;

        let result = {
            let mut state_ref = state.borrow_mut();
            let buf = state_ref.buf.as_mut().expect("buffer went away");

            let start = buf_page * SECTOR_SIZE;
            let end = start + SECTOR_SIZE;
            fd.read_page(&mut buf.as_bytes_mut()[start..end], file_page as _)
        };
        complete_page_op(&state, result, completions);
    }
}
