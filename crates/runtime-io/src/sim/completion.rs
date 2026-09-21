use alloc::{boxed::Box, sync::Arc};
use core::{
    convert::identity,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use crate::{
    sim::{collections::BoundedSlab, fs, Error},
    AlignedBytes, ErasedBox, ErrorWith, Statx,
};

pub use slab::VacantEntry;

pub(crate) enum CompletionState<T> {
    Pending(Option<Waker>),
    Ready(T),
}

impl<T> CompletionState<T> {
    fn complete(&mut self, v: T) -> Option<Waker> {
        match self {
            Self::Pending(waker) => {
                let waker = waker.take();
                *self = CompletionState::Ready(v);
                waker
            }
            Self::Ready(_) => unreachable!("completion completed twice"),
        }
    }
}

pub(super) struct PendingCompletions {
    inner: BoundedSlab<CompletionHandle>,
}

impl PendingCompletions {
    pub(super) fn with_capacity(cap: NonZeroUsize) -> Self {
        Self {
            inner: BoundedSlab::with_capacity(cap),
        }
    }

    pub(super) fn get_mut(&mut self, key: usize) -> Option<&mut CompletionHandle> {
        self.inner.get_mut(key)
    }

    pub(super) fn clear(&mut self) {
        self.inner.clear();
    }

    pub(super) fn vacant_entry(&mut self) -> Option<VacantEntry<'_, CompletionHandle>> {
        self.inner.vacant_entry()
    }

    fn remove(&mut self, key: usize) -> CompletionHandle {
        self.inner.remove(key)
    }

    fn try_remove(&mut self, key: usize) -> Option<CompletionHandle> {
        self.inner.try_remove(key)
    }
}

pub(crate) enum CompletionHandle {
    Write(CompletionState<Result<ErasedBox, ErrorWith<Error, ErasedBox>>>),
    Read(CompletionState<Result<ErasedBox, ErrorWith<Error, ErasedBox>>>),
    Open(CompletionState<Result<fs::File, Error>>),
    Create(CompletionState<Result<fs::File, Error>>),
    Stat(CompletionState<Result<Statx, Error>>),
    Fallocate(CompletionState<Result<(), Error>>),
    Fsync(CompletionState<Result<(), Error>>),
    Fdatasync(CompletionState<Result<(), Error>>),
    #[allow(unused)]
    Noop(CompletionState<Result<(), Error>>),
}

impl CompletionHandle {
    fn write_state_mut(&mut self) -> &mut CompletionState<Result<ErasedBox, ErrorWith<Error, ErasedBox>>> {
        match self {
            Self::Write(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_write_state(self) -> CompletionState<Result<ErasedBox, ErrorWith<Error, ErasedBox>>> {
        match self {
            Self::Write(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_write(&mut self, result: Result<ErasedBox, ErrorWith<Error, ErasedBox>>) -> Option<Waker> {
        self.write_state_mut().complete(result)
    }

    fn read_state_mut(&mut self) -> &mut CompletionState<Result<ErasedBox, ErrorWith<Error, ErasedBox>>> {
        match self {
            Self::Read(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_read_state(self) -> CompletionState<Result<ErasedBox, ErrorWith<Error, ErasedBox>>> {
        match self {
            Self::Read(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_read(&mut self, result: Result<ErasedBox, ErrorWith<Error, ErasedBox>>) -> Option<Waker> {
        self.read_state_mut().complete(result)
    }

    fn open_state_mut(&mut self) -> &mut CompletionState<Result<fs::File, Error>> {
        match self {
            Self::Open(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_open_state(self) -> CompletionState<Result<fs::File, Error>> {
        match self {
            Self::Open(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_open(&mut self, result: Result<fs::File, Error>) -> Option<Waker> {
        self.open_state_mut().complete(result)
    }

    fn create_state_mut(&mut self) -> &mut CompletionState<Result<fs::File, Error>> {
        match self {
            Self::Create(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_create_state(self) -> CompletionState<Result<fs::File, Error>> {
        match self {
            Self::Create(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_create(&mut self, result: Result<fs::File, Error>) -> Option<Waker> {
        self.create_state_mut().complete(result)
    }

    fn stat_state_mut(&mut self) -> &mut CompletionState<Result<Statx, Error>> {
        match self {
            Self::Stat(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_stat_state(self) -> CompletionState<Result<Statx, Error>> {
        match self {
            Self::Stat(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_stat(&mut self, result: Result<Statx, Error>) -> Option<Waker> {
        self.stat_state_mut().complete(result)
    }

    fn fallocate_state_mut(&mut self) -> &mut CompletionState<Result<(), Error>> {
        match self {
            Self::Fallocate(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_fallocate_state(self) -> CompletionState<Result<(), Error>> {
        match self {
            Self::Fallocate(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_fallocate(&mut self, result: Result<(), Error>) -> Option<Waker> {
        self.fallocate_state_mut().complete(result)
    }

    fn fsync_state_mut(&mut self) -> &mut CompletionState<Result<(), Error>> {
        match self {
            Self::Fsync(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_fsync_state(self) -> CompletionState<Result<(), Error>> {
        match self {
            Self::Fsync(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_fsync(&mut self, result: Result<(), Error>) -> Option<Waker> {
        self.fsync_state_mut().complete(result)
    }

    fn fdatasync_state_mut(&mut self) -> &mut CompletionState<Result<(), Error>> {
        match self {
            Self::Fdatasync(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_fdatasync_state(self) -> CompletionState<Result<(), Error>> {
        match self {
            Self::Fdatasync(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_fdatasync(&mut self, result: Result<(), Error>) -> Option<Waker> {
        self.fdatasync_state_mut().complete(result)
    }

    fn noop_state_mut(&mut self) -> &mut CompletionState<Result<(), Error>> {
        match self {
            Self::Noop(state) => state,
            _ => unreachable!(),
        }
    }

    fn into_noop_state(self) -> CompletionState<Result<(), Error>> {
        match self {
            Self::Noop(state) => state,
            _ => unreachable!(),
        }
    }

    pub(crate) fn complete_noop(&mut self, result: Result<(), Error>) -> Option<Waker> {
        self.noop_state_mut().complete(result)
    }
}

pub struct Completion<T> {
    inner: CompletionInner<T>,
}

impl<T> Completion<T> {
    pub(super) fn ready(val: T) -> Self {
        CompletionInner::Ready(Some(val)).into()
    }
}

impl<T: AlignedBytes + Send + 'static> Completion<Result<Box<T>, ErrorWith<Error, Box<T>>>> {
    pub(super) fn write(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::write_state_mut,
                    CompletionHandle::into_write_state,
                    reify,
                    cx,
                )
            },
        }
        .into()
    }

    pub(super) fn read(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::read_state_mut,
                    CompletionHandle::into_read_state,
                    reify,
                    cx,
                )
            },
        }
        .into()
    }
}

impl Completion<Result<fs::File, Error>> {
    pub(super) fn open(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::open_state_mut,
                    CompletionHandle::into_open_state,
                    identity,
                    cx,
                )
            },
        }
        .into()
    }

    pub(super) fn create(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::create_state_mut,
                    CompletionHandle::into_create_state,
                    identity,
                    cx,
                )
            },
        }
        .into()
    }
}

impl Completion<Result<Statx, Error>> {
    pub(super) fn stat(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::stat_state_mut,
                    CompletionHandle::into_stat_state,
                    identity,
                    cx,
                )
            },
        }
        .into()
    }
}

impl Completion<Result<(), Error>> {
    pub(super) fn fallocate(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::fallocate_state_mut,
                    CompletionHandle::into_fallocate_state,
                    identity,
                    cx,
                )
            },
        }
        .into()
    }

    pub(super) fn fsync(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::fsync_state_mut,
                    CompletionHandle::into_fsync_state,
                    identity,
                    cx,
                )
            },
        }
        .into()
    }

    pub(super) fn fdatasync(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::fdatasync_state_mut,
                    CompletionHandle::into_fdatasync_state,
                    identity,
                    cx,
                )
            },
        }
        .into()
    }

    #[allow(unused)]
    pub(super) fn noop(pending: Arc<spin::Mutex<PendingCompletions>>, key: usize) -> Self {
        CompletionInner::Poll {
            pending,
            key,
            poll: |pending, key, cx| {
                poll_completion(
                    pending,
                    key,
                    CompletionHandle::noop_state_mut,
                    CompletionHandle::into_noop_state,
                    identity,
                    cx,
                )
            },
        }
        .into()
    }
}

impl<T> From<CompletionInner<T>> for Completion<T> {
    fn from(inner: CompletionInner<T>) -> Self {
        Self { inner }
    }
}

/// Dropping a [Completion] future removes the [CompletionHandle] from the
/// pending list, if it is present.
///
/// If it is not present, then the future was polled to completion already.
enum CompletionInner<T> {
    Poll {
        pending: Arc<spin::Mutex<PendingCompletions>>,
        key: usize,
        poll: fn(spin::MutexGuard<'_, PendingCompletions>, usize, &mut Context<'_>) -> Poll<T>,
    },
    Ready(Option<T>),
}

impl<T> Drop for CompletionInner<T> {
    fn drop(&mut self) {
        let Self::Poll { pending, key, .. } = self else {
            return;
        };
        pending.lock().try_remove(*key);
    }
}

fn poll_completion<S, T>(
    mut pending: spin::MutexGuard<'_, PendingCompletions>,
    key: usize,
    state_mut: fn(&mut CompletionHandle) -> &mut CompletionState<S>,
    into_state: fn(CompletionHandle) -> CompletionState<S>,
    map: fn(S) -> T,
    cx: &mut Context<'_>,
) -> Poll<T> {
    match pending.get_mut(key) {
        None => unreachable!("completion polled after already complete"),
        Some(handle) => {
            if let CompletionState::Pending(maybe_waker) = (state_mut)(handle) {
                if !maybe_waker.as_ref().is_some_and(|waker| waker.will_wake(cx.waker())) {
                    *maybe_waker = Some(cx.waker().clone());
                }

                return Poll::Pending;
            }

            let handle = pending.remove(key);
            let state = (into_state)(handle);

            match state {
                CompletionState::Ready(result) => Poll::Ready((map)(result)),
                CompletionState::Pending(_) => unreachable!("pending case already handled"),
            }
        }
    }
}

impl<T> Unpin for Completion<T> {}

impl<T> Future for Completion<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match &mut this.inner {
            CompletionInner::Poll { pending, key, poll } => poll(pending.lock(), *key, cx),
            CompletionInner::Ready(val) => match val.take() {
                Some(val) => Poll::Ready(val),
                None => Poll::Pending,
            },
        }
    }
}

fn reify<T: AlignedBytes + Send + 'static>(
    result: Result<ErasedBox, ErrorWith<Error, ErasedBox>>,
) -> Result<Box<T>, ErrorWith<Error, Box<T>>> {
    match result {
        Ok(erased) => Ok(erased.into_aligned::<T>()),
        Err(ErrorWith { error, with }) => Err(ErrorWith {
            error,
            with: with.into_aligned::<T>(),
        }),
    }
}
