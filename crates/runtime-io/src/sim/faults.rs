use core::{convert::Infallible, num::NonZeroUsize};

use crate::sim::{
    executor::{FsyncEffect, InFlight, ReadSector, WriteSector},
    fs::Datasync,
    Error,
};

/// Interface to inject [Fault]s into the simulator.
///
/// Submissions are split into one or more operations, or effects, each
/// executing independently and in any order (see [TaskSelector]). For example,
/// a write that spans multiple sectors will yield one atomic write per sector.
///
/// Before executing an effect, the simulator calls the corresponding `inject_*`
/// method on the fault injector. This allows the implementor to skip, delay or
/// fail the effect. Where applicable, the effect can also be modified: for
/// example, a sector write can be made misdirected by returning a [WriteSector]
/// with a different `sector` from [FaultInjector::inject_write_sector_fault].
///
/// A [FaultInjector] can be stateful, which is why the methods take a mutable
/// `self` reference. If desired, effects can be correlated with submissions via
/// the [InFlight] reference passed to the `inject_*` methods.
pub trait FaultInjector<UserData> {
    fn inject_write_sector_fault(&mut self, _: &InFlight<UserData>, op: WriteSector) -> Fault<WriteSector> {
        Fault::Visible(Effect::Run(op))
    }

    fn inject_read_sector_fault(&mut self, _: &InFlight<UserData>, op: ReadSector) -> Fault<ReadSector> {
        Fault::Visible(Effect::Run(op))
    }

    fn inject_open_fault(&mut self, _: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }

    fn inject_create_fault(&mut self, _: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }

    fn inject_stat_fault(&mut self, _: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }

    fn inject_fallocate_fault(&mut self, _: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }

    fn inject_fsync_fault(&mut self, _: &InFlight<UserData>, op: FsyncEffect) -> Fault<FsyncEffect> {
        Fault::Visible(Effect::Run(op))
    }

    fn inject_fdatasync_fault(&mut self, _: &InFlight<UserData>, op: Datasync) -> Fault<Datasync> {
        Fault::Visible(Effect::Run(op))
    }

    fn inject_noop_fault(&mut self, _: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }
}

/// The unit [FaultInjector] injects no faults.
impl<UserData> FaultInjector<UserData> for () {}

pub enum Fault<T> {
    /// Drop the operation entirely.
    ///
    /// Note that this is not generally possible in `io-uring`: an SQE always
    /// yields a CQE, even if it was cancelled or returned an error. It may be
    /// useful occasionally to construct "byzantine" failures.
    Skip,
    /// Put the operation back onto the queue for later execution.
    Delay(T),
    /// Execute a visible effect.
    Visible(Effect<T>),
}

impl<T> Fault<T> {
    pub(super) fn exec_visible(self, f: impl FnOnce(EitherOrBoth<T, Error>)) -> Option<T> {
        match self {
            Fault::Skip => None,
            Fault::Delay(effect) => Some(effect),
            Fault::Visible(visible) => {
                visible.exec(f);
                None
            }
        }
    }
}

pub enum Effect<T> {
    /// Run the operation as normal.
    Run(T),
    /// Run the effect, but report an injected error.
    RunThenError { effect: T, error: Error },
    /// Skip the effect, but report an injected error.
    SkipThenError { error: Error },
}

impl<T> Effect<T> {
    fn exec(self, f: impl FnOnce(EitherOrBoth<T, Error>)) {
        use EitherOrBoth::*;
        match self {
            Effect::Run(effect) => f(Left(effect)),
            Effect::RunThenError { effect, error } => f(Both(effect, error)),
            Effect::SkipThenError { error } => f(Right(error)),
        }
    }
}

pub(super) enum EitherOrBoth<T, U> {
    Left(T),
    Right(U),
    Both(T, U),
}

impl<T, U> EitherOrBoth<T, U> {
    pub(super) fn traverse<V>(self, f: impl FnOnce(T) -> V, g: impl FnOnce(U) -> V) -> V {
        match self {
            Self::Left(t) => f(t),
            Self::Right(u) => g(u),
            Self::Both(t, u) => {
                f(t);
                g(u)
            }
        }
    }
}

/// Interface to perturb execution ordering.
///
/// Submissions are split into one or more operations, or effects, each
/// executing independently. For example, a write that spans multiple sectors
/// will yield one atomic write per sector.
///
/// Effects could be executed by the kernel in any order, and this trait allows
/// to inject this ordering deterministically (e.g. using a deterministic random
/// number generator).
pub trait TaskSelector {
    type IndexSelector<'a>: IndexSelector
    where
        Self: 'a;

    /// Of `task_count` queued tasks, select the ones to run.
    ///
    /// Called on each [crate::sim::Executor::tick].
    fn select_tasks(&self, task_count: NonZeroUsize) -> TaskSelection<Self::IndexSelector<'_>>;
}

/// Task selection.
pub enum TaskSelection<S> {
    /// Run `count` tasks in FIFO order.
    Fifo { count: NonZeroUsize },
    /// Select `count` tasks by calling `select` `count` times.
    Any { count: NonZeroUsize, select: S },
}

/// Select an index in a range.
pub trait IndexSelector {
    /// Select a number in the range `0..range_upper`.
    fn select_index(&mut self, range_upper: NonZeroUsize) -> usize;
}

impl IndexSelector for Infallible {
    fn select_index(&mut self, _: NonZeroUsize) -> usize {
        match *self {}
    }
}

/// [TaskSelector] that runs all queued tasks in FIFO order.
pub struct FifoAll;

impl TaskSelector for FifoAll {
    type IndexSelector<'a>
        = Infallible
    where
        Self: 'a;

    fn select_tasks(&self, count: NonZeroUsize) -> TaskSelection<Self::IndexSelector<'_>> {
        TaskSelection::Fifo { count }
    }
}

/// [TaskSelector] that runs one task at a time, in FIFO order.
pub struct FifoOne;

impl TaskSelector for FifoOne {
    type IndexSelector<'a>
        = Infallible
    where
        Self: 'a;

    fn select_tasks(&self, _: NonZeroUsize) -> TaskSelection<Self::IndexSelector<'_>> {
        TaskSelection::Fifo {
            count: NonZeroUsize::MIN,
        }
    }
}
