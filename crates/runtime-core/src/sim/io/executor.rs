#![allow(unused)]

use alloc::{
    boxed::Box,
    collections::{btree_map, BTreeMap, VecDeque},
    vec::Vec,
};
use core::{num::NonZeroUsize, result::Result};
use slab::Slab;

use crate::{
    io::{ErasedBoxPtr, Statx, SECTOR_SIZE},
    sim::{
        io::{fs, Error, Instant},
        Rng,
    },
};

/// Dependency on the previous [Sqe].
///
/// A link imposes an ordering constraint: the [Sqe] carrying the link will not
/// be executed before the preceding one completed. Note that a link is only
/// meaningful within a batch of SQEs submitted together.
#[derive(Clone, Copy)]
pub enum LinkKind {
    /// If the preceding SQE failed, cancel this SQE with [Error::Cancelled].
    /// Analogous to `IOSQE_IO_LINK`.
    Soft,
    /// Run the SQE regardless of the preceding SQE's result.
    /// Analoguous to `IOSQE_IO_HARDLINK`.
    Hard,
}

pub struct Sqe<T> {
    inner: SqeInner,
    link: Option<LinkKind>,
    user_data: Option<T>,
}

impl<T> Sqe<T> {
    pub fn link(mut self, kind: Option<LinkKind>) -> Self {
        self.link = kind;
        self
    }

    pub fn is_linked(&self) -> bool {
        self.link.is_some()
    }

    pub fn attach(mut self, user_data: T) -> Self {
        self.user_data.replace(user_data);
        self
    }

    pub fn write(fd: fs::File, buf: ErasedBoxPtr, offset: u64) -> Self {
        Write { fd, buf, offset }.into()
    }

    pub fn read(fd: fs::File, buf: ErasedBoxPtr, offset: u64) -> Self {
        Read { fd, buf, offset }.into()
    }

    pub fn open(path: impl AsRef<str>) -> Self {
        Open {
            path: path.as_ref().into(),
        }
        .into()
    }

    pub fn create(path: impl AsRef<str>) -> Self {
        Create {
            path: path.as_ref().into(),
        }
        .into()
    }

    pub fn stat(fd: fs::File) -> Self {
        Stat { fd }.into()
    }

    pub fn fallocate(fd: fs::File, len: u64) -> Self {
        Fallocate { fd, total_len: len }.into()
    }

    pub fn fsync(fd: fs::File) -> Self {
        Fsync { fd }.into()
    }

    pub fn fdatasync(fd: fs::File) -> Self {
        Fdatasync { fd }.into()
    }

    pub fn noop() -> Self {
        SqeInner::Noop.into()
    }
}

impl<T, U: Into<SqeInner>> From<U> for Sqe<T> {
    fn from(inner: U) -> Self {
        Self {
            inner: inner.into(),
            link: None,
            user_data: None,
        }
    }
}

enum SqeInner {
    Write(Write),
    Read(Read),
    Open(Open),
    Create(Create),
    Stat(Stat),
    Fallocate(Fallocate),
    Fsync(Fsync),
    Fdatasync(Fdatasync),
    Noop,
}

impl SqeInner {
    fn cancel<T>(self, user_data: Option<T>) -> Cqe<T> {
        match self {
            SqeInner::Write(..) => Cqe::Write {
                result: Err(Error::Cancelled),
                user_data,
            },
            SqeInner::Read(..) => Cqe::Read {
                result: Err(Error::Cancelled),
                user_data,
            },
            SqeInner::Open(..) => Cqe::Open {
                result: Err(Error::Cancelled),
                user_data,
            },
            SqeInner::Create(..) => Cqe::Create {
                result: Err(Error::Cancelled),
                user_data,
            },
            SqeInner::Stat(..) => Cqe::Stat {
                result: Err(Error::Cancelled),
                user_data,
            },
            SqeInner::Fallocate(..) => Cqe::Fallocate {
                result: Err(Error::Cancelled),
                user_data,
            },
            SqeInner::Fsync(..) => Cqe::Fsync {
                result: Err(Error::Cancelled),
                user_data,
            },
            SqeInner::Fdatasync(..) => Cqe::Fdatasync {
                result: Err(Error::Cancelled),
                user_data,
            },
            SqeInner::Noop => Cqe::Noop {
                result: Err(Error::Cancelled),
                user_data,
            },
        }
    }

    fn schedule(self, sqe_id: SqeId) -> (InFlightInner, Vec<Operation>) {
        match self {
            SqeInner::Write(mut sqe) => {
                let Write { buf, offset, .. } = &mut sqe;
                let buf_len = buf.as_bytes().len();
                let first_sector = (*offset / SECTOR_SIZE as u64) as usize;
                let page_count = buf_len / SECTOR_SIZE;

                let ops = (0..page_count)
                    .map(|page| Operation::WriteSector {
                        sqe: sqe_id,
                        page_offset: first_sector + page,
                        buf_offset: *offset as usize + (page * SECTOR_SIZE),
                    })
                    .collect::<Vec<_>>();
                let op_count = ops.len();
                let write = InFlightInner::Write {
                    sqe,
                    op_count,
                    results: Vec::with_capacity(op_count),
                };

                (write, ops)
            }
            SqeInner::Read(mut sqe) => {
                let Read { buf, offset, .. } = &mut sqe;
                let buf_len = buf.as_bytes().len();
                let first_sector = (*offset / SECTOR_SIZE as u64) as usize;
                let page_count = buf_len / SECTOR_SIZE;

                let ops = (0..page_count)
                    .map(|page| Operation::ReadSector {
                        sqe: sqe_id,
                        page_offset: first_sector + page,
                        buf_offset: *offset as usize + (page * SECTOR_SIZE),
                    })
                    .collect::<Vec<_>>();
                let op_count = ops.len();
                let read = InFlightInner::Read {
                    sqe,
                    op_count,
                    results: Vec::with_capacity(op_count),
                };

                (read, ops)
            }
            SqeInner::Open(sqe) => (
                InFlightInner::Open { sqe },
                alloc::vec![Operation::Open { sqe: sqe_id }],
            ),
            SqeInner::Create(sqe) => (
                InFlightInner::Create { sqe },
                alloc::vec![Operation::Create { sqe: sqe_id }],
            ),
            SqeInner::Stat(sqe) => (
                InFlightInner::Stat { sqe },
                alloc::vec![Operation::Stat { sqe: sqe_id }],
            ),
            SqeInner::Fallocate(sqe) => (
                InFlightInner::Fallocate { sqe },
                alloc::vec![Operation::Fallocate { sqe: sqe_id }],
            ),
            SqeInner::Fsync(sqe) => (
                InFlightInner::Fsync { sqe },
                alloc::vec![Operation::Fsync { sqe: sqe_id }],
            ),
            SqeInner::Fdatasync(sqe) => (
                InFlightInner::Fdatasync { sqe },
                alloc::vec![Operation::Fdatasync { sqe: sqe_id }],
            ),
            SqeInner::Noop => (InFlightInner::Noop, alloc::vec![Operation::Noop { sqe: sqe_id }]),
        }
    }
}

impl From<Write> for SqeInner {
    fn from(inner: Write) -> Self {
        Self::Write(inner)
    }
}

impl From<Read> for SqeInner {
    fn from(inner: Read) -> Self {
        Self::Read(inner)
    }
}

impl From<Open> for SqeInner {
    fn from(inner: Open) -> Self {
        Self::Open(inner)
    }
}

impl From<Create> for SqeInner {
    fn from(inner: Create) -> Self {
        Self::Create(inner)
    }
}

impl From<Stat> for SqeInner {
    fn from(inner: Stat) -> Self {
        Self::Stat(inner)
    }
}

impl From<Fallocate> for SqeInner {
    fn from(inner: Fallocate) -> Self {
        Self::Fallocate(inner)
    }
}

impl From<Fsync> for SqeInner {
    fn from(inner: Fsync) -> Self {
        Self::Fsync(inner)
    }
}

impl From<Fdatasync> for SqeInner {
    fn from(inner: Fdatasync) -> Self {
        Self::Fdatasync(inner)
    }
}

struct Write {
    fd: fs::File,
    buf: ErasedBoxPtr,
    offset: u64,
}

struct Read {
    fd: fs::File,
    buf: ErasedBoxPtr,
    offset: u64,
}

struct Open {
    path: Box<str>,
}

struct Create {
    path: Box<str>,
}

struct Stat {
    fd: fs::File,
}

struct Fallocate {
    fd: fs::File,
    total_len: u64,
}

struct Fsync {
    #[allow(unused)]
    fd: fs::File,
}

struct Fdatasync {
    #[allow(unused)]
    fd: fs::File,
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

type SqeId = usize;

enum Operation {
    WriteSector {
        sqe: SqeId,
        page_offset: usize,
        buf_offset: usize,
    },
    ReadSector {
        sqe: SqeId,
        page_offset: usize,
        buf_offset: usize,
    },
    Open {
        sqe: SqeId,
    },
    Create {
        sqe: SqeId,
    },
    Stat {
        sqe: SqeId,
    },
    Fallocate {
        sqe: SqeId,
    },
    Fsync {
        sqe: SqeId,
    },
    Fdatasync {
        sqe: SqeId,
    },
    Noop {
        sqe: SqeId,
    },
}

struct Blocked<T> {
    link: LinkKind,
    sqe: SqeInner,
    user_data: Option<T>,
}

struct InFlight<T> {
    inner: InFlightInner,
    blocked: VecDeque<Blocked<T>>,
    user_data: Option<T>,
}

enum InFlightInner {
    Write {
        sqe: Write,
        op_count: usize,
        results: Vec<fs::Result<()>>,
    },
    Read {
        sqe: Read,
        op_count: usize,
        results: Vec<fs::Result<()>>,
    },
    Open {
        sqe: Open,
    },
    Create {
        sqe: Create,
    },
    Stat {
        sqe: Stat,
    },
    Fallocate {
        sqe: Fallocate,
    },
    Fsync {
        sqe: Fsync,
    },
    Fdatasync {
        sqe: Fdatasync,
    },
    Noop,
}

pub enum WriteFault {
    /// Misdirect the write to an arbitrary page offset in the file.
    Misdirected { page_offset: usize },
    /// Report the write as successful, but don't write anything.
    Lost,
    /// Report the write as successful, but write less bytes than requested.
    Short { write_bytes: usize },
    /// Delay the write until at least `deadline`.
    Delayed { deadline: Instant },
    /// Execute the side effects, but never report completion.
    NoCompletion,
    /// Report an error without executing side effects.
    Error(Error),
}

pub trait FaultInjector {
    fn maybe_write_fault(&self, rng: &Rng, now: Instant, page_offset: usize) -> Option<WriteFault>;
}

/// Completion queue overflow policy.
///
/// Note that we do **not** model `IORING_FEAT_NODROP`, because we never want
/// the application to rely on dynamic memory allocation in the kernel.
///
/// The default is to panic, which should prompt the user to adjust queue size
/// configuration. However, sometimes it may be useful to see how the
/// application behaves when completions are dropped.
#[derive(Default)]
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
    executing: VecDeque<Operation>,

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

    pub fn dropped_completions(&self) -> usize {
        self.cq_dropped
    }

    pub fn completed(&mut self) -> impl Iterator<Item = Cqe<UserData>> {
        self.completions.drain(..)
    }

    pub fn tick(&mut self, rng: &Rng, now: Instant) -> bool {
        let mut progress = self.schedule();
        progress |= self.execute(rng, now);
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
            let (in_flight, ops) = sqe.inner.schedule(slot.key());
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

    fn execute(&mut self, rng: &Rng, now: Instant) -> bool {
        if self.executing.is_empty() {
            return false;
        }
        if let Some(op) = self.executing.remove(rng.index(self.executing.len())) {
            match op {
                Operation::WriteSector {
                    sqe,
                    page_offset,
                    buf_offset,
                } => {
                    let is_complete = {
                        let InFlight {
                            inner:
                                InFlightInner::Write {
                                    sqe: Write { fd, buf, .. },
                                    op_count,
                                    results,
                                },
                            ..
                        } = self.in_flight.get_mut(sqe).expect("invalid sqe id")
                        else {
                            unreachable!("invalid sqe: expected write")
                        };
                        let bytes = buf.as_bytes();
                        let end = (buf_offset + SECTOR_SIZE).min(bytes.len());

                        let buf = &buf.as_bytes()[buf_offset..end];
                        let result = fd.write_page(buf, page_offset as _);
                        results.push(result);

                        results.len() == *op_count
                    };

                    if is_complete {
                        let InFlight {
                            inner:
                                InFlightInner::Write {
                                    sqe: Write { mut buf, .. },
                                    op_count,
                                    results,
                                },
                            blocked,
                            user_data,
                        } = self.in_flight.remove(sqe)
                        else {
                            unreachable!("invalid sqe: expected write")
                        };
                        assert!(results.len() == op_count);
                        // TODO: Propagate all errors?
                        // TODO: Allow write op failures and reflect in returned number.
                        let result = match results.into_iter().find_map(|r| r.map_err(Error::from).err()) {
                            Some(error) => Err(error),
                            None => Ok(buf.as_bytes().len()),
                        };
                        let is_success = result.is_ok();
                        self.complete(Cqe::Write { result, user_data });
                        self.schedule_linked(sqe, is_success, blocked);
                    }
                }
                Operation::ReadSector {
                    sqe,
                    page_offset,
                    buf_offset,
                } => {
                    let is_complete = {
                        let InFlight {
                            inner:
                                InFlightInner::Read {
                                    sqe: Read { fd, buf, .. },
                                    op_count,
                                    results,
                                },
                            ..
                        } = self.in_flight.get_mut(sqe).expect("invalid sqe id")
                        else {
                            unreachable!("invalid sqe: expected read")
                        };
                        let bytes = buf.as_bytes_mut();
                        let end = (buf_offset + SECTOR_SIZE).min(bytes.len());

                        let buf = &mut buf.as_bytes_mut()[buf_offset..end];
                        let result = fd.read_page(buf, page_offset as _);
                        results.push(result);

                        results.len() == *op_count
                    };

                    if is_complete {
                        let InFlight {
                            inner:
                                InFlightInner::Read {
                                    sqe: Read { mut buf, .. },
                                    op_count,
                                    results,
                                },
                            blocked,
                            user_data,
                        } = self.in_flight.remove(sqe)
                        else {
                            unreachable!("invalid sqe: expected read")
                        };
                        assert!(results.len() == op_count);
                        // TODO: Propagate all errors?
                        // TODO: Allow write op failures and reflect in returned number.
                        let result = match results.into_iter().find_map(|r| r.map_err(Error::from).err()) {
                            Some(error) => Err(error),
                            None => Ok(buf.as_bytes().len()),
                        };
                        let is_success = result.is_ok();
                        self.complete(Cqe::Read { result, user_data });
                        self.schedule_linked(sqe, is_success, blocked);
                    }
                }
                Operation::Open { sqe } => {
                    let InFlight {
                        inner: InFlightInner::Open { sqe: Open { path } },
                        blocked,
                        user_data,
                    } = self.in_flight.remove(sqe)
                    else {
                        unreachable!("invalid sqe: expected open")
                    };

                    let result = self.fstree.get(&path).cloned().ok_or(Error::FileNotFound { path });
                    let is_success = result.is_ok();
                    self.complete(Cqe::Open { result, user_data });
                    self.schedule_linked(sqe, is_success, blocked);
                }
                Operation::Create { sqe } => {
                    let InFlight {
                        inner: InFlightInner::Create { sqe: Create { path } },
                        blocked,
                        user_data,
                    } = self.in_flight.remove(sqe)
                    else {
                        unreachable!("invalid sqe: expected create")
                    };

                    let result = match self.fstree.entry(path) {
                        btree_map::Entry::Vacant(entry) => Ok(entry.insert(fs::File::new()).clone()),
                        btree_map::Entry::Occupied(entry) => Err(Error::FileAlreadyExists {
                            path: entry.key().clone(),
                        }),
                    };
                    let is_success = result.is_ok();
                    self.complete(Cqe::Create { result, user_data });
                    self.schedule_linked(sqe, is_success, blocked);
                }
                Operation::Stat { sqe } => {
                    let InFlight {
                        inner: InFlightInner::Stat { sqe: Stat { fd } },
                        blocked,
                        user_data,
                    } = self.in_flight.remove(sqe)
                    else {
                        unreachable!("invalid sqe: expected stat")
                    };

                    self.complete(Cqe::Stat {
                        result: Ok(Statx { size: fd.len() }),
                        user_data,
                    });
                    self.schedule_linked(sqe, true, blocked);
                }
                Operation::Fallocate { sqe } => {
                    let InFlight {
                        inner:
                            InFlightInner::Fallocate {
                                sqe: Fallocate { fd, total_len },
                            },
                        blocked,
                        user_data,
                    } = self.in_flight.remove(sqe)
                    else {
                        unreachable!("invalid sqe: expected fallocate")
                    };

                    self.complete(Cqe::Fallocate {
                        result: fd.set_len(total_len).map_err(Error::from),
                        user_data,
                    });
                    self.schedule_linked(sqe, true, blocked);
                }
                Operation::Fsync { sqe } => {
                    // TODO: Do something fallible with fd.
                    let InFlight {
                        inner: InFlightInner::Fsync { sqe: Fsync { fd: _ } },
                        blocked,
                        user_data,
                    } = self.in_flight.remove(sqe)
                    else {
                        unreachable!("invalid sqe: expected fsync")
                    };

                    self.complete(Cqe::Fsync {
                        result: Ok(()),
                        user_data,
                    });
                    self.schedule_linked(sqe, true, blocked);
                }
                Operation::Fdatasync { sqe } => {
                    // TODO: Do something fallible with fd.
                    let InFlight {
                        inner:
                            InFlightInner::Fdatasync {
                                sqe: Fdatasync { fd: _ },
                            },
                        blocked,
                        user_data,
                    } = self.in_flight.remove(sqe)
                    else {
                        unreachable!("invalid sqe: expected fdatasync")
                    };

                    self.complete(Cqe::Fdatasync {
                        result: Ok(()),
                        user_data,
                    });
                    self.schedule_linked(sqe, true, blocked);
                }
                Operation::Noop { sqe } => {
                    let InFlight {
                        inner: InFlightInner::Noop,
                        blocked,
                        user_data,
                    } = self.in_flight.remove(sqe)
                    else {
                        unreachable!("invalid sqe: expected noop")
                    };

                    self.complete(Cqe::Noop {
                        result: Ok(()),
                        user_data,
                    });
                    self.schedule_linked(sqe, true, blocked);
                }
            }

            return true;
        }

        false
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
                    let slot = self.in_flight.get_mut(sqe).expect("invalid sqe id");
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
