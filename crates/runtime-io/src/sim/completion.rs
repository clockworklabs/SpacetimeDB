use core::{
    convert::identity,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use alloc::sync::Arc;

use crate::{
    sim::{fs, Error, SimulatorInner},
    AlignedBytes, Cancelled, ErasedBox, ErrorWith, Statx,
};

pub(crate) enum CompletionState<T> {
    Pending(Option<Waker>),
    Ready(T),
    Abandoned,
}

impl<T> CompletionState<T> {
    pub(crate) fn complete(&mut self, v: T) -> Option<Waker> {
        match self {
            Self::Pending(waker) => {
                let waker = waker.take();
                *self = CompletionState::Ready(v);
                waker
            }
            Self::Abandoned => None,
            Self::Ready(_) => unreachable!("completion completed twice"),
        }
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

    #[allow(unused)]
    fn noop_state_mut(&mut self) -> &mut CompletionState<Result<(), Error>> {
        match self {
            Self::Noop(state) => state,
            _ => unreachable!(),
        }
    }

    #[allow(unused)]
    fn into_noop_state(self) -> CompletionState<Result<(), Error>> {
        match self {
            Self::Noop(state) => state,
            _ => unreachable!(),
        }
    }
}

pub struct Completion<T> {
    sim: Arc<SimulatorInner>,
    key: usize,
    poll: fn(&SimulatorInner, usize, &mut Context<'_>) -> Poll<Result<T, Cancelled>>,
    drop: fn(&SimulatorInner, usize),
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
            drop: |sim, key| drop_completion(sim, key, CompletionHandle::write_state_mut),
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

            drop: |sim, key| drop_completion(sim, key, CompletionHandle::read_state_mut),
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

            drop: |sim, key| drop_completion(sim, key, CompletionHandle::open_state_mut),
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

            drop: |sim, key| drop_completion(sim, key, CompletionHandle::create_state_mut),
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

            drop: |sim, key| drop_completion(sim, key, CompletionHandle::stat_state_mut),
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
            drop: |sim, key| drop_completion(sim, key, CompletionHandle::fallocate_state_mut),
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
            drop: |sim, key| drop_completion(sim, key, CompletionHandle::fsync_state_mut),
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
            drop: |sim, key| drop_completion(sim, key, CompletionHandle::fdatasync_state_mut),
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
            drop: |sim, key| drop_completion(sim, key, CompletionHandle::noop_state_mut),
        }
    }
}

impl<T> Drop for Completion<T> {
    fn drop(&mut self) {
        (self.drop)(&self.sim, self.key)
    }
}

fn drop_completion<T>(
    sim: &SimulatorInner,
    key: usize,
    state_mut: fn(&mut CompletionHandle) -> &mut CompletionState<T>,
) {
    let mut pending = sim.pending.lock();
    if let Some(handle) = pending.get_mut(key) {
        let state = (state_mut)(handle);
        match state {
            CompletionState::Ready(_) => {
                pending.remove(key);
            }
            CompletionState::Pending(_) | CompletionState::Abandoned => {
                *state = CompletionState::Abandoned;
            }
        }
    }
}

fn poll_completion<S, T>(
    sim: &SimulatorInner,
    key: usize,
    state_mut: fn(&mut CompletionHandle) -> &mut CompletionState<S>,
    into_state: fn(CompletionHandle) -> CompletionState<S>,
    map: fn(S) -> T,
    cx: &mut Context<'_>,
) -> Poll<Result<T, Cancelled>> {
    let mut pending = sim.pending.lock();
    let state = (state_mut)(&mut pending[key]);
    match state {
        CompletionState::Pending(waker) => {
            if !waker.as_ref().is_some_and(|waker| waker.will_wake(cx.waker())) {
                *waker = Some(cx.waker().clone());
            }

            Poll::Pending
        }
        CompletionState::Ready(_result) => {
            let handle = pending.remove(key);
            let state = (into_state)(handle);

            match state {
                CompletionState::Ready(result) => Poll::Ready(Ok((map)(result))),
                _ => unreachable!(),
            }
        }
        CompletionState::Abandoned => Poll::Ready(Err(Cancelled)),
    }
}

impl<T> Future for Completion<T> {
    type Output = Result<T, Cancelled>;

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
