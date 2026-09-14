use core::{
    convert::identity,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use alloc::sync::Arc;
use slab::Slab;

use crate::{
    sim::{fs, Error, SimulatorInner},
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

#[derive(Default)]
pub(super) struct PendingCompletions {
    inner: Slab<CompletionHandle>,
}

impl PendingCompletions {
    pub(super) fn get_mut(&mut self, key: usize) -> Option<&mut CompletionHandle> {
        self.inner.get_mut(key)
    }

    pub(super) fn clear(&mut self) {
        self.inner.clear();
    }

    pub(super) fn vacant_entry(&mut self) -> VacantEntry<'_, CompletionHandle> {
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
    sim: Arc<SimulatorInner>,
    key: usize,
    poll: fn(&SimulatorInner, usize, &mut Context<'_>) -> Poll<T>,
}

impl<T: AlignedBytes + 'static> Completion<Result<T, ErrorWith<Error, T>>> {
    pub(super) fn write(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::write_state_mut,
                    CompletionHandle::into_write_state,
                    reify,
                    cx,
                )
            },
        }
    }

    pub(super) fn read(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::read_state_mut,
                    CompletionHandle::into_read_state,
                    reify,
                    cx,
                )
            },
        }
    }
}

impl Completion<Result<fs::File, Error>> {
    pub(super) fn open(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::open_state_mut,
                    CompletionHandle::into_open_state,
                    identity,
                    cx,
                )
            },
        }
    }

    pub(super) fn create(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::create_state_mut,
                    CompletionHandle::into_create_state,
                    identity,
                    cx,
                )
            },
        }
    }
}

impl Completion<Result<Statx, Error>> {
    pub(super) fn stat(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::stat_state_mut,
                    CompletionHandle::into_stat_state,
                    identity,
                    cx,
                )
            },
        }
    }
}

impl Completion<Result<(), Error>> {
    pub(super) fn fallocate(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::fallocate_state_mut,
                    CompletionHandle::into_fallocate_state,
                    identity,
                    cx,
                )
            },
        }
    }

    pub(super) fn fsync(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::fsync_state_mut,
                    CompletionHandle::into_fsync_state,
                    identity,
                    cx,
                )
            },
        }
    }

    pub(super) fn fdatasync(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::fdatasync_state_mut,
                    CompletionHandle::into_fdatasync_state,
                    identity,
                    cx,
                )
            },
        }
    }

    #[allow(unused)]
    pub(super) fn noop(sim: Arc<SimulatorInner>, key: usize) -> Self {
        Self {
            sim,
            key,
            poll: |sim, key, cx| {
                poll_completion(
                    sim,
                    key,
                    CompletionHandle::noop_state_mut,
                    CompletionHandle::into_noop_state,
                    identity,
                    cx,
                )
            },
        }
    }
}

/// Dropping a [Completion] future removes the [CompletionHandle] from the
/// pending list, if it is present.
///
/// If it is not present, then the future was polled to completion already.
impl<T> Drop for Completion<T> {
    fn drop(&mut self) {
        self.sim.pending.lock().try_remove(self.key);
    }
}

fn poll_completion<S, T>(
    sim: &SimulatorInner,
    key: usize,
    state_mut: fn(&mut CompletionHandle) -> &mut CompletionState<S>,
    into_state: fn(CompletionHandle) -> CompletionState<S>,
    map: fn(S) -> T,
    cx: &mut Context<'_>,
) -> Poll<T> {
    let mut pending = sim.pending.lock();
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
                _ => unreachable!(),
            }
        }
    }
}

impl<T> Future for Completion<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        (this.poll)(&this.sim, this.key, cx)
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
