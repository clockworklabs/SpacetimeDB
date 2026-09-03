use alloc::{boxed::Box, vec::Vec};

use crate::{
    io::{ErasedBoxPtr, SECTOR_SIZE},
    sim::io::{
        executor::{Cqe, Executing, FsyncEffect, InFlightInner, Operation, ReadSector, WriteSector},
        fs::{self, Datasync},
        Error,
    },
};

/// Opaque identifier of a scheduled [Sqe].
#[derive(Clone, Copy)]
pub struct SqeId(pub(super) usize);

impl SqeId {
    pub(super) fn key(&self) -> usize {
        self.0
    }
}

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
    pub(super) inner: SqeInner,
    pub(super) link: Option<LinkKind>,
    pub(super) user_data: Option<T>,
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

pub enum SqeInner {
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
    pub(super) fn cancel<T>(self, user_data: Option<T>) -> Cqe<T> {
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

    pub(super) fn schedule(self, sqe_id: SqeId) -> (InFlightInner, Vec<Executing>) {
        match self {
            SqeInner::Write(mut sqe) => {
                let Write { buf, offset, .. } = &mut sqe;
                let buf_len = buf.as_bytes().len();
                let first_sector = (*offset / SECTOR_SIZE as u64) as usize;
                let page_count = buf_len / SECTOR_SIZE;

                let ops = (0..page_count)
                    .map(|page| Executing {
                        sqe: sqe_id,
                        inner: Operation::WriteSector(WriteSector {
                            page_offset: first_sector + page,
                            buf_offset: *offset as usize + (page * SECTOR_SIZE),
                        }),
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
                    .map(|page| Executing {
                        sqe: sqe_id,
                        inner: Operation::ReadSector(ReadSector {
                            page_offset: first_sector + page,
                            buf_offset: *offset as usize + (page * SECTOR_SIZE),
                        }),
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
                alloc::vec![Executing {
                    sqe: sqe_id,
                    inner: Operation::Open
                }],
            ),
            SqeInner::Create(sqe) => (
                InFlightInner::Create { sqe },
                alloc::vec![Executing {
                    sqe: sqe_id,
                    inner: Operation::Create
                }],
            ),
            SqeInner::Stat(sqe) => (
                InFlightInner::Stat { sqe },
                alloc::vec![Executing {
                    sqe: sqe_id,
                    inner: Operation::Stat
                }],
            ),
            SqeInner::Fallocate(sqe) => (
                InFlightInner::Fallocate { sqe },
                alloc::vec![Executing {
                    sqe: sqe_id,
                    inner: Operation::Fallocate
                }],
            ),
            SqeInner::Fsync(sqe) => {
                let Fsync { fd } = &sqe;

                let sector_count = fd.len() / SECTOR_SIZE as u64;
                let ops = (0..sector_count)
                    .map(|offset| Executing {
                        sqe: sqe_id,
                        inner: Operation::Fsync {
                            effect: FsyncEffect::Datasync(Datasync::Sector(offset)),
                        },
                    })
                    .chain([Executing {
                        sqe: sqe_id,
                        inner: Operation::Fsync {
                            effect: FsyncEffect::Datasync(Datasync::Length),
                        },
                    }])
                    .collect::<Vec<_>>();
                let op_count = ops.len();
                let in_flight = InFlightInner::Fsync {
                    sqe,
                    op_count,
                    results: Vec::with_capacity(op_count),
                };

                (in_flight, ops)
            }
            SqeInner::Fdatasync(sqe) => {
                let Fdatasync { fd } = &sqe;

                let sector_count = fd.len() / SECTOR_SIZE as u64;
                let ops = (0..sector_count)
                    .map(|offset| Executing {
                        sqe: sqe_id,
                        inner: Operation::Fdatasync {
                            effect: Datasync::Sector(offset),
                        },
                    })
                    .chain([Executing {
                        sqe: sqe_id,
                        inner: Operation::Fdatasync {
                            effect: Datasync::Length,
                        },
                    }])
                    .collect::<Vec<_>>();
                let op_count = ops.len();
                let in_flight = InFlightInner::Fdatasync {
                    sqe,
                    op_count,
                    results: Vec::with_capacity(op_count),
                };

                (in_flight, ops)
            }
            SqeInner::Noop => (
                InFlightInner::Noop,
                alloc::vec![Executing {
                    sqe: sqe_id,
                    inner: Operation::Noop
                }],
            ),
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

pub struct Write {
    pub fd: fs::File,
    pub buf: ErasedBoxPtr,
    pub offset: u64,
}

pub struct Read {
    pub fd: fs::File,
    pub buf: ErasedBoxPtr,
    pub offset: u64,
}

pub struct Open {
    pub path: Box<str>,
}

pub struct Create {
    pub path: Box<str>,
}

pub struct Stat {
    pub fd: fs::File,
}

pub struct Fallocate {
    pub fd: fs::File,
    pub total_len: u64,
}

pub struct Fsync {
    pub fd: fs::File,
}

pub struct Fdatasync {
    pub fd: fs::File,
}
