use alloc::boxed::Box;

use crate::{
    sim::{
        executor::{Cqe, CqeInner, FsyncEffect, Operation, Pending, ReadSector, Results, WriteSector},
        fs::{self, Datasync},
        Error,
    },
    ErasedBox, SECTOR_SIZE, SECTOR_SIZE64,
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
    pub(crate) inner: SqeInner,
    pub(super) link: Option<LinkKind>,
    pub(super) user_data: Option<T>,
}

impl<T> Sqe<T> {
    fn new(inner: SqeInner) -> Self {
        Self {
            inner,
            link: None,
            user_data: None,
        }
    }

    #[allow(unused)]
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

    pub fn write(fd: fs::File, buf: ErasedBox, offset: u64) -> Self {
        assert!(offset.is_multiple_of(SECTOR_SIZE64));
        Self::new(SqeInner::Write { fd, buf, offset })
    }

    pub fn read(fd: fs::File, buf: ErasedBox, offset: u64) -> Self {
        assert!(offset.is_multiple_of(SECTOR_SIZE64));
        Self::new(SqeInner::Read { fd, buf, offset })
    }

    pub fn open(path: Box<str>) -> Self {
        Self::new(SqeInner::Open { path })
    }

    pub fn create(path: Box<str>) -> Self {
        Self::new(SqeInner::Create { path })
    }

    pub fn stat(fd: fs::File) -> Self {
        Self::new(SqeInner::Stat { fd })
    }

    pub fn fallocate(fd: fs::File, len: u64) -> Self {
        Self::new(SqeInner::Fallocate { fd, total_len: len })
    }

    pub fn fsync(fd: fs::File) -> Self {
        Self::new(SqeInner::Fsync { fd })
    }

    pub fn fdatasync(fd: fs::File) -> Self {
        Self::new(SqeInner::Fdatasync { fd })
    }

    #[allow(unused)]
    pub fn noop() -> Self {
        Self::new(SqeInner::Noop)
    }

    /// Extract the [ErasedBox] buffer if the [Sqe] carries one.
    pub(crate) fn into_buf(self) -> Option<ErasedBox> {
        match self.inner {
            SqeInner::Write { buf, .. } | SqeInner::Read { buf, .. } => Some(buf),
            SqeInner::Open { .. }
            | SqeInner::Create { .. }
            | SqeInner::Stat { .. }
            | SqeInner::Fallocate { .. }
            | SqeInner::Fsync { .. }
            | SqeInner::Fdatasync { .. }
            | SqeInner::Noop => None,
        }
    }
}

pub enum SqeInner {
    Write { fd: fs::File, buf: ErasedBox, offset: u64 },
    Read { fd: fs::File, buf: ErasedBox, offset: u64 },
    Open { path: Box<str> },
    Create { path: Box<str> },
    Stat { fd: fs::File },
    Fallocate { fd: fs::File, total_len: u64 },
    Fsync { fd: fs::File },
    Fdatasync { fd: fs::File },
    Noop,
}

impl SqeInner {
    pub(super) fn cancel<T>(self, user_data: Option<T>) -> Cqe<T> {
        match self {
            SqeInner::Write { buf, .. } => Cqe {
                inner: CqeInner::Write {
                    result: Err(Error::Cancelled),
                    buf,
                },
                user_data,
            },
            SqeInner::Read { buf, .. } => Cqe {
                inner: CqeInner::Read {
                    result: Err(Error::Cancelled),
                    buf,
                },
                user_data,
            },
            SqeInner::Open { .. } => Cqe {
                inner: CqeInner::Open {
                    result: Err(Error::Cancelled),
                },
                user_data,
            },
            SqeInner::Create { .. } => Cqe {
                inner: CqeInner::Create {
                    result: Err(Error::Cancelled),
                },
                user_data,
            },
            SqeInner::Stat { .. } => Cqe {
                inner: CqeInner::Stat {
                    result: Err(Error::Cancelled),
                },
                user_data,
            },
            SqeInner::Fallocate { .. } => Cqe {
                inner: CqeInner::Fallocate {
                    result: Err(Error::Cancelled),
                },
                user_data,
            },
            SqeInner::Fsync { .. } => Cqe {
                inner: CqeInner::Fsync {
                    result: Err(Error::Cancelled),
                },
                user_data,
            },
            SqeInner::Fdatasync { .. } => Cqe {
                inner: CqeInner::Fdatasync {
                    result: Err(Error::Cancelled),
                },
                user_data,
            },
            SqeInner::Noop => Cqe {
                inner: CqeInner::Noop {
                    result: Err(Error::Cancelled),
                },
                user_data,
            },
        }
    }

    pub(super) fn prepare(&self) -> Pending {
        match self {
            SqeInner::Write { buf, offset, .. } => {
                let buf_len = buf.as_bytes().len();
                let first_sector = (*offset / SECTOR_SIZE64) as usize;
                let sector_count = buf_len / SECTOR_SIZE;

                Pending::ReadWrite {
                    ops: (0..sector_count).scan(first_sector, |first_sector, sector| {
                        Some(Operation::WriteSector(WriteSector {
                            sector: *first_sector + sector,
                            buf_offset: sector * SECTOR_SIZE,
                        }))
                    }),
                    results: Results::new(sector_count),
                }
            }
            SqeInner::Read { buf, offset, .. } => {
                let buf_len = buf.as_bytes().len();
                let first_sector = (*offset / SECTOR_SIZE64) as usize;
                let sector_count = buf_len / SECTOR_SIZE;

                Pending::ReadWrite {
                    ops: (0..sector_count).scan(first_sector, |first_sector, sector| {
                        Some(Operation::ReadSector(ReadSector {
                            sector: *first_sector + sector,
                            buf_offset: sector * SECTOR_SIZE,
                        }))
                    }),
                    results: Results::new(sector_count),
                }
            }
            SqeInner::Fsync { fd } => {
                let sector_count = fd.len() / SECTOR_SIZE64;
                Pending::Sync {
                    ops: fd.prepare_datasync().map(
                        (|effect| Operation::Fsync {
                            effect: FsyncEffect::Datasync(effect),
                        }) as fn(Datasync) -> Operation,
                    ),
                    results: Results::new(1 + sector_count as usize),
                }
            }
            SqeInner::Fdatasync { fd } => {
                let sector_count = fd.len() / SECTOR_SIZE64;
                Pending::Sync {
                    ops: fd.prepare_datasync().map(|effect| Operation::Fdatasync { effect }),
                    results: Results::new(1 + sector_count as usize),
                }
            }
            SqeInner::Open { .. } => Pending::Unit {
                op: Some(Operation::Open),
            },
            SqeInner::Create { .. } => Pending::Unit {
                op: Some(Operation::Create),
            },
            SqeInner::Stat { .. } => Pending::Unit {
                op: Some(Operation::Stat),
            },
            SqeInner::Fallocate { .. } => Pending::Unit {
                op: Some(Operation::Fallocate),
            },
            SqeInner::Noop => Pending::Unit {
                op: Some(Operation::Noop),
            },
        }
    }
}
