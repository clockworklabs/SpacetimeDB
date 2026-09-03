#![allow(unused)]

use alloc::{
    boxed::Box,
    collections::{btree_map, BTreeMap, VecDeque},
    vec::Vec,
};
use core::{mem, num::NonZeroUsize, result::Result};
use slab::Slab;

use crate::{
    io::{ErasedBoxPtr, Statx, SECTOR_SIZE},
    sim::{
        io::{fs, Error, Instant},
        Rng,
    },
};

pub use crate::sim::io::fs::Datasync;

mod sqe;
use sqe::SqeInner;
pub use sqe::{LinkKind, Sqe, SqeId};

// TODO: There is no difference between fsync and fdatasync as long as we don't
// have an API to fsync the directory of a file after it was created.
#[derive(Clone, Copy)]
pub enum FsyncEffect {
    Datasync(Datasync),
}

#[derive(Clone, Copy)]
pub enum Operation {
    WriteSector(WriteSector),
    ReadSector(ReadSector),
    Open,
    Create,
    Stat,
    Fallocate,
    Fsync { effect: FsyncEffect },
    Fdatasync { effect: Datasync },
    Noop,
}

#[derive(Clone, Copy)]
pub struct WriteSector {
    pub page_offset: usize,
    pub buf_offset: usize,
}

#[derive(Clone, Copy)]
pub struct ReadSector {
    pub page_offset: usize,
    pub buf_offset: usize,
}

#[derive(Debug)]
pub enum Cqe<T> {
    Write {
        result: Result<usize, Error>,
        user_data: Option<T>,
    },
    Read {
        result: Result<usize, Error>,
        user_data: Option<T>,
    },
    Open {
        result: Result<fs::File, Error>,
        user_data: Option<T>,
    },
    Create {
        result: Result<fs::File, Error>,
        user_data: Option<T>,
    },
    Stat {
        result: Result<Statx, Error>,
        user_data: Option<T>,
    },
    Fallocate {
        result: Result<(), Error>,
        user_data: Option<T>,
    },
    Fsync {
        result: Result<(), Error>,
        user_data: Option<T>,
    },
    Fdatasync {
        result: Result<(), Error>,
        user_data: Option<T>,
    },
    Noop {
        result: Result<(), Error>,
        user_data: Option<T>,
    },
}

impl<T> Cqe<T> {
    pub fn user_data(&self) -> &Option<T> {
        match self {
            Self::Write { user_data, .. }
            | Self::Read { user_data, .. }
            | Self::Open { user_data, .. }
            | Self::Create { user_data, .. }
            | Self::Stat { user_data, .. }
            | Self::Fallocate { user_data, .. }
            | Self::Fsync { user_data, .. }
            | Self::Fdatasync { user_data, .. }
            | Self::Noop { user_data, .. } => user_data,
        }
    }
}

pub struct Blocked<T> {
    pub link: LinkKind,
    pub sqe: SqeInner,
    pub user_data: Option<T>,
}

pub struct InFlight<T> {
    pub inner: InFlightInner,
    pub blocked: VecDeque<Blocked<T>>,
    pub user_data: Option<T>,
}

pub enum InFlightInner {
    Write {
        sqe: sqe::Write,
        op_count: usize,
        results: Vec<Result<(), Error>>,
    },
    Read {
        sqe: sqe::Read,
        op_count: usize,
        results: Vec<Result<(), Error>>,
    },
    Open {
        sqe: sqe::Open,
    },
    Create {
        sqe: sqe::Create,
    },
    Stat {
        sqe: sqe::Stat,
    },
    Fallocate {
        sqe: sqe::Fallocate,
    },
    Fsync {
        sqe: sqe::Fsync,
        op_count: usize,
        results: Vec<Result<(), Error>>,
    },
    Fdatasync {
        sqe: sqe::Fdatasync,
        op_count: usize,
        results: Vec<Result<(), Error>>,
    },
    Noop,
}

struct Executing {
    sqe: SqeId,
    inner: Operation,
}

impl Executing {
    fn traverse(self, f: impl FnOnce(SqeId, Operation) -> Option<Operation>) -> Option<Self> {
        let Self { sqe, inner } = self;
        f(sqe, inner).map(|inner| Self { sqe, inner })
    }
}

pub enum Fault<T> {
    /// Drop the operation entirely.
    Skip,
    /// Put the operation back onto the queue for later execution.
    Delay(T),
    /// Execute a visible effect.
    Visible(Effect<T>),
}

impl<T> Fault<T> {
    fn exec_visible(self, f: impl FnOnce(EitherOrBoth<T, Error>)) -> Option<T> {
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

enum EitherOrBoth<T, U> {
    Left(T),
    Right(U),
    Both(T, U),
}

impl<T, U> EitherOrBoth<T, U> {
    fn traverse<V>(self, f: impl FnOnce(T) -> V, g: impl FnOnce(U) -> V) -> V {
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

pub trait FaultInjector<UserData> {
    fn inject_write_sector_fault(&mut self, sqe: &InFlight<UserData>, op: WriteSector) -> Fault<WriteSector> {
        Fault::Visible(Effect::Run(op))
    }

    fn inject_read_sector_fault(&mut self, sqe: &InFlight<UserData>, op: ReadSector) -> Fault<ReadSector> {
        Fault::Visible(Effect::Run(op))
    }

    fn inject_open_fault(&mut self, sqe: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }

    fn inject_create_fault(&mut self, sqe: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }

    fn inject_stat_fault(&mut self, sqe: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }

    fn inject_fallocate_fault(&mut self, sqe: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }

    fn inject_fsync_fault(&mut self, sqe: &InFlight<UserData>, op: FsyncEffect) -> Fault<FsyncEffect> {
        Fault::Visible(Effect::Run(op))
    }

    fn inject_fdatasync_fault(&mut self, sqe: &InFlight<UserData>, op: Datasync) -> Fault<Datasync> {
        Fault::Visible(Effect::Run(op))
    }

    fn inject_noop_fault(&mut self, sqe: &InFlight<UserData>) -> Fault<()> {
        Fault::Visible(Effect::Run(()))
    }
}

pub struct NoFaults;
impl<UserData> FaultInjector<UserData> for NoFaults {}

/// Completion queue overflow policy.
///
/// Note that we do **not** model `IORING_FEAT_NODROP`, because we never want
/// the application to rely on dynamic memory allocation in the kernel.
///
/// The default is to panic, which should prompt the user to adjust queue size
/// configuration. However, sometimes it may be useful to see how the
/// application behaves when completions are dropped.
#[derive(Clone, Copy, Default)]
pub enum OnCqOverflow {
    #[default]
    Panic,
    Drop,
}

pub struct Options {
    /// Capacity of the submission queue.
    ///
    /// This basically limits how many [Sqe]s can be submitted in one batch.
    /// Should be a power of 2, or is otherwise rounded up to the next power of
    /// 2.
    pub capacity: NonZeroUsize,
    /// Override the completion queue capacity.
    ///
    /// By default, the completion queue's capacity is twice the submission
    /// queue's. This can be insufficient for some workloads, so this setting
    /// can be used to override the default.
    ///
    /// Should be a power of 2, or is otherwise rounder up to the next power of
    /// two.
    pub cq_capacity: Option<NonZeroUsize>,
    /// What to do if the completion queue overflows.
    pub cq_overflow: OnCqOverflow,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            capacity: NonZeroUsize::new(8).unwrap(),
            cq_capacity: None,
            cq_overflow: OnCqOverflow::default(),
        }
    }
}

pub struct Executor<UserData> {
    submissions: VecDeque<Sqe<UserData>>,
    completions: VecDeque<Cqe<UserData>>,

    in_flight: Slab<InFlight<UserData>>,
    executing: VecDeque<Executing>,

    fstree: BTreeMap<Box<str>, fs::File>,

    cq_overflow: OnCqOverflow,
    cq_dropped: usize,
}

impl<UserData> Executor<UserData> {
    pub fn new(
        Options {
            capacity,
            cq_capacity,
            cq_overflow,
        }: Options,
    ) -> Self {
        let sq_capacity = capacity.get().next_power_of_two();
        let cq_capacity = cq_capacity
            .map(|c| c.get().next_power_of_two())
            .unwrap_or_else(|| 2 * sq_capacity);
        Self {
            submissions: VecDeque::with_capacity(sq_capacity),
            completions: VecDeque::with_capacity(cq_capacity),
            in_flight: Slab::new(),
            executing: VecDeque::new(),
            fstree: BTreeMap::new(),
            cq_overflow,
            cq_dropped: 0,
        }
    }

    /// Simulate a power-loss crash.
    ///
    /// All submitted and executing operations are cancelled, and files reset to
    /// their durable state. After this method returns, the completion queue is
    /// empty.
    pub fn crash(&mut self) {
        self.submissions.clear();
        self.completions.clear();
        self.in_flight.clear();
        self.executing.clear();
        self.cq_dropped = 0;

        for file in self.fstree.values_mut() {
            file.crash();
        }
    }

    /// Restart the executor, simulating a process crash.
    ///
    /// Unlike [Self::crash], this will drive the currently executing operations
    /// to completion. Submissions that were not yet scheduled are dropped. The
    /// file state remains unchanged.
    ///
    /// Execution is subject to `faults`. If a fault evaluates to [Fault::Skip],
    /// that operation is dropped.
    ///
    /// After this method returns, the completion queue is empty.
    pub fn restart(&mut self, faults: &mut impl FaultInjector<UserData>) {
        self.submissions.clear();
        let cq_overflow_orig = self.cq_overflow;
        self.cq_overflow = OnCqOverflow::Drop;
        let executing = mem::take(&mut self.executing);
        for op in executing {
            self.execute(op, faults);
        }
        self.completions.clear();
        self.cq_overflow = cq_overflow_orig;
        self.cq_dropped = 0;
    }

    /// Submit a batch of [Sqe]s for later execution.
    pub fn submit<Batch>(&mut self, sqes: Batch) -> Result<(), Batch::IntoIter>
    where
        Batch: IntoIterator<Item = Sqe<UserData>>,
        Batch::IntoIter: ExactSizeIterator,
    {
        let sqes = sqes.into_iter();
        if self.submissions.len() + sqes.len() >= self.submissions.capacity() {
            Err(sqes)
        } else {
            self.submissions.extend(sqes);
            Ok(())
        }
    }

    fn complete(&mut self, cqe: Cqe<UserData>) {
        if self.completions.len() == self.completions.capacity() {
            match self.cq_overflow {
                OnCqOverflow::Panic => panic!("completion queue overflow"),
                OnCqOverflow::Drop => {
                    self.cq_dropped += 1;
                    return;
                }
            }
        }
        self.completions.push_back(cqe);
    }

    /// Number of completions that were dropped due to completion queue overflow
    /// over the lifetime of this executor.
    ///
    /// Always zero if the executor was configured with [OnCqOverflow::Panic].
    pub fn dropped_completions(&self) -> usize {
        self.cq_dropped
    }

    /// Drain the completion queue.
    pub fn completed(&mut self) -> impl Iterator<Item = Cqe<UserData>> {
        self.completions.drain(..)
    }

    /// Drain the submission queue and advance one scheduled operation.
    ///
    /// The operation to advance is chosen randomly using `rng`.
    /// The operation is subject to `faults`.
    pub fn tick(&mut self, rng: &Rng, faults: &mut impl FaultInjector<UserData>) -> bool {
        let mut progress = self.schedule();
        progress |= self.execute_random(rng, faults);
        progress
    }

    fn schedule(&mut self) -> bool {
        let mut progress = false;

        while let Some(sqe) = self.submissions.pop_front() {
            // If the sqe is linked, pop the whole chain.
            // Links of sqes not submitted in the same batch are ignored.
            let mut successors = VecDeque::new();
            if let Some(link) = sqe.link {
                let mut link_kind = link;
                while let Some(Sqe { inner, link, user_data }) = self.submissions.pop_front() {
                    successors.push_back(Blocked {
                        link: link_kind,
                        sqe: inner,
                        user_data,
                    });
                    match link {
                        Some(kind) => link_kind = kind,
                        None => break,
                    }
                }
            }
            let slot = self.in_flight.vacant_entry();
            let (in_flight, ops) = sqe.inner.schedule(SqeId(slot.key()));
            self.executing.extend(ops);
            slot.insert(InFlight {
                inner: in_flight,
                blocked: successors,
                user_data: sqe.user_data,
            });

            progress = true
        }

        progress
    }

    fn execute_random(&mut self, rng: &Rng, faults: &mut impl FaultInjector<UserData>) -> bool {
        if self.executing.is_empty() {
            return false;
        }
        if let Some(op) = self.executing.remove(rng.index(self.executing.len())) {
            if let Some(delay) = self.execute(op, faults) {
                self.executing.push_back(delay);
            }
            true
        } else {
            false
        }
    }

    fn execute(&mut self, op: Executing, faults: &mut impl FaultInjector<UserData>) -> Option<Executing> {
        op.traverse(|sqe, op| {
            let in_flight = self.in_flight.get(sqe.key()).expect("invalid sqe id");
            match op {
                Operation::WriteSector(effect) => faults
                    .inject_write_sector_fault(in_flight, effect)
                    .exec_visible(|eff| self.execute_write_sector(sqe, eff))
                    .map(Operation::WriteSector),
                Operation::ReadSector(effect) => faults
                    .inject_read_sector_fault(in_flight, effect)
                    .exec_visible(|eff| self.execute_read_sector(sqe, eff))
                    .map(Operation::ReadSector),
                Operation::Open => faults
                    .inject_open_fault(in_flight)
                    .exec_visible(|eff| self.execute_open(sqe, eff))
                    .map(|()| Operation::Open),
                Operation::Create => faults
                    .inject_create_fault(in_flight)
                    .exec_visible(|eff| self.execute_create(sqe, eff))
                    .map(|()| Operation::Create),
                Operation::Stat => faults
                    .inject_stat_fault(in_flight)
                    .exec_visible(|eff| self.execute_stat(sqe, eff))
                    .map(|()| Operation::Stat),
                Operation::Fallocate => faults
                    .inject_fallocate_fault(in_flight)
                    .exec_visible(|eff| self.execute_fallocate(sqe, eff))
                    .map(|()| Operation::Fallocate),
                Operation::Fsync { effect } => faults
                    .inject_fsync_fault(in_flight, effect)
                    .exec_visible(|eff| self.execute_fsync(sqe, eff))
                    .map(|effect| Operation::Fsync { effect }),
                Operation::Fdatasync { effect } => faults
                    .inject_fdatasync_fault(in_flight, effect)
                    .exec_visible(|eff| self.execute_fdatasync(sqe, eff))
                    .map(|effect| Operation::Fdatasync { effect }),
                Operation::Noop => faults
                    .inject_noop_fault(in_flight)
                    .exec_visible(|eff| self.execute_noop(sqe, eff))
                    .map(|()| Operation::Noop),
            }
        })
    }

    fn execute_write_sector(&mut self, sqe: SqeId, eff: EitherOrBoth<WriteSector, Error>) {
        let is_complete = {
            let InFlight {
                inner:
                    InFlightInner::Write {
                        sqe: sqe::Write { fd, buf, .. },
                        op_count,
                        results,
                    },
                ..
            } = self.in_flight.get_mut(sqe.key()).expect("invalid sqe id")
            else {
                unreachable!("invalid sqe: expected write")
            };
            let mut run = |WriteSector {
                               page_offset,
                               buf_offset,
                           }| {
                let bytes = buf.as_bytes();
                let end = (buf_offset + SECTOR_SIZE).min(bytes.len());

                let buf = &buf.as_bytes()[buf_offset..end];
                fd.write_page(buf, page_offset as _).map_err(Into::into)
            };
            results.push(eff.traverse(run, Err));

            results.len() == *op_count
        };

        if is_complete {
            let InFlight {
                inner:
                    InFlightInner::Write {
                        sqe: sqe::Write { mut buf, .. },
                        op_count,
                        results,
                    },
                blocked,
                user_data,
            } = self.in_flight.remove(sqe.key())
            else {
                unreachable!("invalid sqe: expected write")
            };
            assert!(results.len() == op_count);
            let bytes_written = results.iter().filter(|r| r.is_ok()).count() * SECTOR_SIZE;
            // TODO: Propagate all errors?
            let result = match results.into_iter().find_map(Result::err) {
                Some(error) => Err(error),
                None => Ok(bytes_written),
            };
            let is_success = result.is_ok();
            self.complete(Cqe::Write { result, user_data });
            self.schedule_linked(sqe, is_success, blocked);
        }
    }

    fn execute_read_sector(&mut self, sqe: SqeId, eff: EitherOrBoth<ReadSector, Error>) {
        let is_complete = {
            let InFlight {
                inner:
                    InFlightInner::Read {
                        sqe: sqe::Read { fd, buf, .. },
                        op_count,
                        results,
                    },
                ..
            } = self.in_flight.get_mut(sqe.key()).expect("invalid sqe id")
            else {
                unreachable!("invalid sqe: expected read")
            };
            let mut run = |ReadSector {
                               page_offset,
                               buf_offset,
                           }| {
                let bytes = buf.as_bytes_mut();
                let end = (buf_offset + SECTOR_SIZE).min(bytes.len());

                let buf = &mut buf.as_bytes_mut()[buf_offset..end];
                fd.read_page(buf, page_offset as _).map_err(Into::into)
            };
            results.push(eff.traverse(run, Err));

            results.len() == *op_count
        };

        if is_complete {
            let InFlight {
                inner:
                    InFlightInner::Read {
                        sqe: sqe::Read { mut buf, .. },
                        op_count,
                        results,
                    },
                blocked,
                user_data,
            } = self.in_flight.remove(sqe.key())
            else {
                unreachable!("invalid sqe: expected read")
            };
            assert!(results.len() == op_count);
            let bytes_read = results.iter().filter(|r| r.is_ok()).count() * SECTOR_SIZE;
            // TODO: Propagate all errors?
            let result = match results.into_iter().find_map(Result::err) {
                Some(error) => Err(error),
                None => Ok(bytes_read),
            };
            let is_success = result.is_ok();
            self.complete(Cqe::Read { result, user_data });
            self.schedule_linked(sqe, is_success, blocked);
        }
    }

    fn execute_open(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            inner: InFlightInner::Open {
                sqe: sqe::Open { path },
            },
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected open")
        };
        let result = eff.traverse(
            |()| self.fstree.get(&path).cloned().ok_or(Error::FileNotFound { path }),
            Err,
        );

        let is_success = result.is_ok();
        self.complete(Cqe::Open { result, user_data });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn execute_create(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            inner: InFlightInner::Create {
                sqe: sqe::Create { path },
            },
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected create")
        };
        let run = |()| match self.fstree.entry(path) {
            btree_map::Entry::Vacant(entry) => Ok(entry.insert(fs::File::new()).clone()),
            btree_map::Entry::Occupied(entry) => Err(Error::FileAlreadyExists {
                path: entry.key().clone(),
            }),
        };
        let result = eff.traverse(run, Err);
        let is_success = result.is_ok();
        self.complete(Cqe::Create { result, user_data });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn execute_stat(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            inner: InFlightInner::Stat { sqe: sqe::Stat { fd } },
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected stat")
        };
        let result = eff.traverse(|()| Ok(Statx { size: fd.len() }), Err);
        let is_success = result.is_ok();
        self.complete(Cqe::Stat { result, user_data });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn execute_fallocate(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            inner: InFlightInner::Fallocate {
                sqe: sqe::Fallocate { fd, total_len },
            },
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected fallocate")
        };
        let result = eff.traverse(|()| fd.set_len(total_len).map_err(Into::into), Err);
        let is_success = result.is_ok();
        self.complete(Cqe::Fallocate { result, user_data });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn execute_fsync(&mut self, sqe: SqeId, eff: EitherOrBoth<FsyncEffect, Error>) {
        let is_complete = {
            let InFlight {
                inner:
                    InFlightInner::Fsync {
                        sqe: sqe::Fsync { fd },
                        op_count,
                        results,
                    },
                ..
            } = self.in_flight.get_mut(sqe.key()).expect("invalid sqe id")
            else {
                unreachable!("invalid sqe: expected fsync")
            };
            let result = eff.traverse(
                |FsyncEffect::Datasync(effect)| {
                    fd.fdatasync([effect]);
                    Ok(())
                },
                Err,
            );
            results.push(result);

            results.len() == *op_count
        };

        if is_complete {
            let InFlight {
                inner: InFlightInner::Fsync { results, .. },
                blocked,
                user_data,
            } = self.in_flight.remove(sqe.key())
            else {
                unreachable!("invalid sqe: expected fsync")
            };
            // TODO: Propagate all errors?
            let result = results.into_iter().find_map(Result::err).map(Err).unwrap_or(Ok(()));
            let is_success = result.is_ok();
            self.complete(Cqe::Fsync { result, user_data });
            self.schedule_linked(sqe, is_success, blocked);
        }
    }

    fn execute_fdatasync(&mut self, sqe: SqeId, eff: EitherOrBoth<Datasync, Error>) {
        let is_complete = {
            let InFlight {
                inner:
                    InFlightInner::Fdatasync {
                        sqe: sqe::Fdatasync { fd },
                        op_count,
                        results,
                    },
                ..
            } = self.in_flight.get_mut(sqe.key()).expect("invalid sqe id")
            else {
                unreachable!("invalid sqe: expected fdatasync")
            };
            let result = eff.traverse(
                |effect| {
                    fd.fdatasync([effect]);
                    Ok(())
                },
                Err,
            );
            results.push(result);

            results.len() == *op_count
        };

        if is_complete {
            let InFlight {
                inner: InFlightInner::Fdatasync { results, .. },
                blocked,
                user_data,
            } = self.in_flight.remove(sqe.key())
            else {
                unreachable!("invalid sqe: expected fdatasync")
            };
            // TODO: Propagate all errors?
            let result = results.into_iter().find_map(Result::err).map(Err).unwrap_or(Ok(()));
            let is_success = result.is_ok();
            self.complete(Cqe::Fdatasync { result, user_data });
            self.schedule_linked(sqe, is_success, blocked);
        }
    }

    fn execute_noop(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            inner: InFlightInner::Noop,
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected noop")
        };
        let result = eff.traverse(Ok, Err);
        let is_success = result.is_ok();
        self.complete(Cqe::Noop { result, user_data });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn schedule_linked(&mut self, sqe: SqeId, prev_succeeded: bool, mut blocked: VecDeque<Blocked<UserData>>) {
        if let Some(Blocked {
            link,
            sqe: next,
            user_data,
        }) = blocked.pop_front()
        {
            match (link, prev_succeeded) {
                (LinkKind::Soft, false) => {
                    self.complete(next.cancel(user_data));
                    for Blocked {
                        link: _,
                        sqe: next,
                        user_data,
                    } in blocked
                    {
                        self.complete(next.cancel(user_data));
                    }
                }
                (LinkKind::Soft, true) | (LinkKind::Hard, _) => {
                    let (inner, ops) = next.schedule(sqe);
                    self.executing.extend(ops);
                    let slot = self.in_flight.get_mut(sqe.key()).expect("invalid sqe id");
                    *slot = InFlight {
                        inner,
                        blocked,
                        user_data,
                    };
                }
            }
        }
    }
}
