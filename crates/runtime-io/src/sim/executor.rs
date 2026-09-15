use alloc::{
    boxed::Box,
    collections::{btree_map, BTreeMap, VecDeque},
    vec::Vec,
};
use core::{num::NonZeroUsize, result::Result, task::Waker};
use slab::Slab;

use crate::{
    sim::{completion::CompletionHandle, fs, Error},
    ErasedBox, ErrorWith, Statx, SECTOR_SIZE,
};

pub use crate::sim::fs::Datasync;

mod sqe;
use sqe::SqeInner;
pub use sqe::{LinkKind, Sqe, SqeId};

pub trait TaskSelector {
    /// Deterministically select zero or more tasks to advance.
    ///
    /// `task_count` is the number of currently outstanding tasks. The returned
    /// iterator must return indexes in the range `0..task_count` and not yield
    /// duplicate elements.
    ///
    /// Called once per [Executor::tick].
    fn select_tasks(&self, task_count: usize) -> impl IntoIterator<Item = usize>;
}

// TODO: There is no difference between fsync and fdatasync until we extend
// [Statx] with additional fields.
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
pub struct Cqe<T> {
    inner: CqeInner,
    user_data: Option<T>,
}
impl<T> Cqe<T> {
    pub fn user_data(&self) -> &Option<T> {
        &self.user_data
    }

    pub(crate) fn complete(self, completion: &mut CompletionHandle) -> Option<Waker> {
        self.inner.complete(completion)
    }
}

#[derive(Debug)]
pub enum CqeInner {
    Write {
        result: Result<usize, Error>,
        buf: ErasedBox,
    },
    Read {
        result: Result<usize, Error>,
        buf: ErasedBox,
    },
    Open {
        result: Result<fs::File, Error>,
    },
    Create {
        result: Result<fs::File, Error>,
    },
    Stat {
        result: Result<Statx, Error>,
    },
    Fallocate {
        result: Result<(), Error>,
    },
    Fsync {
        result: Result<(), Error>,
    },
    Fdatasync {
        result: Result<(), Error>,
    },
    Noop {
        result: Result<(), Error>,
    },
}

impl CqeInner {
    fn complete(self, completion: &mut CompletionHandle) -> Option<Waker> {
        match self {
            Self::Write { result, buf, .. } => {
                let result = match result {
                    Ok(written) if written == buf.len() => Ok(buf),
                    Ok(written) => Err(ErrorWith {
                        error: Error::ShortWrite {
                            expected: buf.len(),
                            written,
                        },
                        with: buf,
                    }),
                    Err(error) => Err(ErrorWith { error, with: buf }),
                };
                completion.complete_write(result)
            }
            Self::Read { result, buf, .. } => {
                let result = match result {
                    Ok(read) if read == buf.len() => Ok(buf),
                    Ok(read) => Err(ErrorWith {
                        error: Error::UnexpectedEof {
                            expected: buf.len(),
                            read,
                        },
                        with: buf,
                    }),
                    Err(error) => Err(ErrorWith { error, with: buf }),
                };
                completion.complete_read(result)
            }
            Self::Open { result, .. } => completion.complete_open(result),
            Self::Create { result, .. } => completion.complete_create(result),
            Self::Stat { result, .. } => completion.complete_stat(result),
            Self::Fallocate { result, .. } => completion.complete_fallocate(result),
            Self::Fsync { result, .. } => completion.complete_fsync(result),
            Self::Fdatasync { result, .. } => completion.complete_fdatasync(result),
            Self::Noop { result, .. } => completion.complete_noop(result),
        }
    }
}

pub struct Blocked<T> {
    pub link: LinkKind,
    pub sqe: SqeInner,
    pub user_data: Option<T>,
}

pub struct InFlight<T> {
    pub sqe: SqeInner,
    pub pending: Pending,
    pub blocked: VecDeque<Blocked<T>>,
    pub user_data: Option<T>,
}

pub trait ResultAcc {
    fn empty() -> Self;
    fn add(&mut self, other: Self);
}

impl ResultAcc for usize {
    fn empty() -> Self {
        0
    }

    fn add(&mut self, other: Self) {
        *self += other
    }
}

impl ResultAcc for () {
    fn empty() -> Self {}
    fn add(&mut self, _: Self) {}
}

pub struct Results<T> {
    remaining: usize,
    value: T,
    error: Option<Error>,
}

impl<T: ResultAcc> Results<T> {
    fn new(op_count: usize) -> Self {
        Self {
            remaining: op_count,
            value: T::empty(),
            error: None,
        }
    }

    fn push(&mut self, res: Result<T, Error>) {
        match res {
            Ok(value) => self.value.add(value),
            Err(e) => {
                self.error.get_or_insert(e);
            }
        }
        self.remaining -= 1;
    }

    fn into_result(self) -> Result<T, Error> {
        assert_eq!(self.remaining, 0);
        self.error.map_or(Ok(self.value), Err)
    }

    fn is_complete(&self) -> bool {
        self.remaining == 0
    }
}

pub enum Pending {
    OneOff,
    ReadWrite { results: Results<usize> },
    Sync { results: Results<()> },
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

impl<UserData> FaultInjector<UserData> for () {}

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
    /// Should be a power of 2, or is otherwise rounded up to the next power of
    /// two.
    pub cq_capacity: Option<NonZeroUsize>,
    /// What to do if the completion queue overflows.
    pub cq_overflow: OnCqOverflow,
    /// Bound on the number of concurrently executing tasks.
    ///
    /// Note that one [Sqe] can result in many operations to be scheduled.
    /// Should be a power of 2, or is otherwise rounded up to the next power of
    /// two.
    pub max_concurrency: NonZeroUsize,
}

impl Options {
    pub(crate) fn sq_capacity(&self) -> usize {
        self.capacity.get().next_power_of_two()
    }

    pub(crate) fn cq_capacity(&self) -> usize {
        self.cq_capacity
            .map(|c| c.get().next_power_of_two())
            .unwrap_or_else(|| 2 * self.sq_capacity())
    }

    pub(crate) fn max_concurrency(&self) -> usize {
        self.max_concurrency.get().next_power_of_two()
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            capacity: NonZeroUsize::new(8).unwrap(),
            cq_capacity: None,
            cq_overflow: OnCqOverflow::default(),
            max_concurrency: NonZeroUsize::new(32).unwrap(),
        }
    }
}

pub struct Executor<UserData> {
    submissions: VecDeque<Sqe<UserData>>,
    completions: VecDeque<Cqe<UserData>>,

    in_flight: Slab<InFlight<UserData>>,
    executing: Vec<Executing>,
    // Scratch space for task selector
    select_executing: Vec<usize>,

    fstree: BTreeMap<Box<str>, fs::File>,

    cq_overflow: OnCqOverflow,
    cq_dropped: usize,
}

impl<UserData> Executor<UserData> {
    pub fn new(options: Options) -> Self {
        let sq_capacity = options.sq_capacity();
        let cq_capacity = options.cq_capacity();
        Self {
            submissions: VecDeque::with_capacity(sq_capacity),
            completions: VecDeque::with_capacity(cq_capacity),
            in_flight: Slab::with_capacity(2 * sq_capacity),
            executing: Vec::with_capacity(options.max_concurrency()),
            select_executing: Vec::with_capacity(options.max_concurrency()),
            fstree: BTreeMap::new(),
            cq_overflow: options.cq_overflow,
            cq_dropped: 0,
        }
    }

    /// Simulate a power-loss crash.
    ///
    /// All submitted and executing operations are cancelled, and files reset to
    /// their durable state. After this method returns, the completion queue is
    /// empty.
    pub fn power_loss(&mut self) {
        self.submissions.clear();
        self.completions.clear();
        self.in_flight.clear();
        self.executing.clear();
        self.cq_dropped = 0;

        for file in self.fstree.values_mut() {
            file.power_loss();
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
        while let Some(op) = self.executing.pop() {
            self.execute_op(op, faults);
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
    #[allow(unused)]
    pub fn dropped_completions(&self) -> usize {
        self.cq_dropped
    }

    /// Drain the completion queue.
    pub fn completed(&mut self) -> impl Iterator<Item = Cqe<UserData>> {
        self.completions.drain(..)
    }

    /// Drain the submission queue and advance one scheduled operation.
    ///
    /// The operation to advance is chosen by `task_selector`.
    /// The operation is subject to `faults`.
    pub fn tick(&mut self, task_selector: &impl TaskSelector, faults: &mut impl FaultInjector<UserData>) -> bool {
        let mut progress = self.schedule();
        progress |= self.execute(task_selector, faults);
        progress
    }

    fn schedule(&mut self) -> bool {
        let mut progress = false;

        while let Some(mut sqe) = self.submissions.pop_front() {
            let slot = self.in_flight.vacant_entry();
            if let Some(pending) = sqe.inner.schedule(SqeId(slot.key()), &mut self.executing) {
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
                slot.insert(InFlight {
                    sqe: sqe.inner,
                    pending,
                    blocked: successors,
                    user_data: sqe.user_data,
                });
            }

            progress = true
        }

        progress
    }

    fn execute(&mut self, task_selector: &impl TaskSelector, faults: &mut impl FaultInjector<UserData>) -> bool {
        let mut progress = false;

        self.select_executing.clear();
        self.select_executing.extend(
            task_selector
                .select_tasks(self.executing.len())
                .into_iter()
                .take(self.executing.len()),
        );
        self.select_executing.sort_unstable_by(|a, b| b.cmp(a));

        let mut prev = None;
        for i in 0..self.select_executing.len() {
            let index = self.select_executing[i];
            assert_ne!(prev, Some(index), "duplicate task selected");
            prev = Some(index);
            let op = self.executing.swap_remove(index);
            if let Some(delay) = self.execute_op(op, faults) {
                self.executing.push(delay);
            }
            progress |= true;
        }

        progress
    }

    fn execute_op(&mut self, op: Executing, faults: &mut impl FaultInjector<UserData>) -> Option<Executing> {
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
                sqe: SqeInner::Write { fd, buf, .. },
                pending: Pending::ReadWrite { results },
                ..
            } = self.in_flight.get_mut(sqe.key()).expect("invalid sqe id")
            else {
                unreachable!("invalid sqe: expected write")
            };
            let run = |WriteSector {
                           page_offset,
                           buf_offset,
                       }| {
                let bytes = buf.as_bytes();
                let end = (buf_offset + SECTOR_SIZE).min(bytes.len());

                let buf = &buf.as_bytes()[buf_offset..end];
                fd.write_page(buf, page_offset as _).map_err(Into::into)
            };
            results.push(eff.traverse(run, Err));
            results.is_complete()
        };

        if is_complete {
            let InFlight {
                sqe: SqeInner::Write { buf, .. },
                pending: Pending::ReadWrite { results },
                blocked,
                user_data,
            } = self.in_flight.remove(sqe.key())
            else {
                unreachable!("invalid sqe: expected write")
            };
            let result = results.into_result();
            let is_success = result.is_ok();
            self.complete(Cqe {
                inner: CqeInner::Write { result, buf },
                user_data,
            });
            self.schedule_linked(sqe, is_success, blocked);
        }
    }

    fn execute_read_sector(&mut self, sqe: SqeId, eff: EitherOrBoth<ReadSector, Error>) {
        let is_complete = {
            let InFlight {
                sqe: SqeInner::Read { fd, buf, .. },
                pending: Pending::ReadWrite { results },
                ..
            } = self.in_flight.get_mut(sqe.key()).expect("invalid sqe id")
            else {
                unreachable!("invalid sqe: expected read")
            };
            let run = |ReadSector {
                           page_offset,
                           buf_offset,
                       }| {
                let bytes = buf.as_bytes_mut();
                let end = (buf_offset + SECTOR_SIZE).min(bytes.len());

                let buf = &mut buf.as_bytes_mut()[buf_offset..end];
                fd.read_page(buf, page_offset as _).map_err(Into::into)
            };
            results.push(eff.traverse(run, Err));
            results.is_complete()
        };

        if is_complete {
            let InFlight {
                sqe: SqeInner::Read { buf, .. },
                pending: Pending::ReadWrite { results },
                blocked,
                user_data,
            } = self.in_flight.remove(sqe.key())
            else {
                unreachable!("invalid sqe: expected read")
            };
            let result = results.into_result();
            let is_success = result.is_ok();
            self.complete(Cqe {
                inner: CqeInner::Read { result, buf },
                user_data,
            });
            self.schedule_linked(sqe, is_success, blocked);
        }
    }

    fn execute_open(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            sqe: SqeInner::Open { path },
            pending: Pending::OneOff,
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
        self.complete(Cqe {
            inner: CqeInner::Open { result },
            user_data,
        });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn execute_create(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            sqe: SqeInner::Create { path },
            pending: Pending::OneOff,
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected create")
        };
        let run = |()| {
            // Avoid cloning `path` if already exists.
            if self.fstree.contains_key(&path) {
                Err(Error::FileAlreadyExists { path })
            } else {
                Ok(self.fstree.entry(path).or_insert_with(fs::File::default).clone())
            }
        };
        let result = eff.traverse(run, Err);
        let is_success = result.is_ok();
        self.complete(Cqe {
            inner: CqeInner::Create { result },
            user_data,
        });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn execute_stat(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            sqe: SqeInner::Stat { fd },
            pending: Pending::OneOff,
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected stat")
        };
        let result = eff.traverse(|()| Ok(Statx { size: fd.len() }), Err);
        let is_success = result.is_ok();
        self.complete(Cqe {
            inner: CqeInner::Stat { result },
            user_data,
        });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn execute_fallocate(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            sqe: SqeInner::Fallocate { fd, total_len },
            pending: Pending::OneOff,
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected fallocate")
        };
        let result = eff.traverse(|()| fd.set_len(total_len).map_err(Into::into), Err);
        let is_success = result.is_ok();
        self.complete(Cqe {
            inner: CqeInner::Fallocate { result },
            user_data,
        });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn execute_fsync(&mut self, sqe: SqeId, eff: EitherOrBoth<FsyncEffect, Error>) {
        let is_complete = {
            let InFlight {
                sqe: SqeInner::Fsync { fd },
                pending: Pending::Sync { results },
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
            results.is_complete()
        };

        if is_complete {
            let InFlight {
                pending: Pending::Sync { results },
                blocked,
                user_data,
                ..
            } = self.in_flight.remove(sqe.key())
            else {
                unreachable!("invalid sqe: expected fsync")
            };
            let result = results.into_result();
            let is_success = result.is_ok();
            self.complete(Cqe {
                inner: CqeInner::Fsync { result },
                user_data,
            });
            self.schedule_linked(sqe, is_success, blocked);
        }
    }

    fn execute_fdatasync(&mut self, sqe: SqeId, eff: EitherOrBoth<Datasync, Error>) {
        let is_complete = {
            let InFlight {
                sqe: SqeInner::Fdatasync { fd },
                pending: Pending::Sync { results },
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
            results.is_complete()
        };

        if is_complete {
            let InFlight {
                pending: Pending::Sync { results },
                blocked,
                user_data,
                ..
            } = self.in_flight.remove(sqe.key())
            else {
                unreachable!("invalid sqe: expected fdatasync")
            };
            let result = results.into_result();
            let is_success = result.is_ok();
            self.complete(Cqe {
                inner: CqeInner::Fdatasync { result },
                user_data,
            });
            self.schedule_linked(sqe, is_success, blocked);
        }
    }

    fn execute_noop(&mut self, sqe: SqeId, eff: EitherOrBoth<(), Error>) {
        let InFlight {
            sqe: SqeInner::Noop,
            pending: Pending::OneOff,
            blocked,
            user_data,
        } = self.in_flight.remove(sqe.key())
        else {
            unreachable!("invalid sqe: expected noop")
        };
        let result = eff.traverse(Ok, Err);
        let is_success = result.is_ok();
        self.complete(Cqe {
            inner: CqeInner::Noop { result },
            user_data,
        });
        self.schedule_linked(sqe, is_success, blocked);
    }

    fn schedule_linked(&mut self, sqe: SqeId, prev_succeeded: bool, mut blocked: VecDeque<Blocked<UserData>>) {
        if let Some(Blocked {
            link,
            sqe: mut next,
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
                    let pending = next.schedule(sqe, &mut self.executing);
                    let slot = self.in_flight.get_mut(sqe.key()).expect("invalid sqe id");
                    *slot = InFlight {
                        sqe: next,
                        pending,
                        blocked,
                        user_data,
                    };
                }
            }
        }
    }
}
