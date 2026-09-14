use alloc::{boxed::Box, collections::vec_deque::VecDeque};

use crate::{
    sim::{
        executor::{Cqe, CqeInner, Executing, FsyncEffect, Operation, Pending, ReadSector, Results, WriteSector},
        fs::{self, Datasync},
        Error,
    },
    ErasedBox, SECTOR_SIZE,
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
    #[allow(unused)]
    pub fn link(mut self, kind: Option<LinkKind>) -> Self {
        self.link = kind;
        self
    }

    #[allow(unused)]
    pub fn is_linked(&self) -> bool {
        self.link.is_some()
    }

    pub fn attach(mut self, user_data: T) -> Self {
        self.user_data.replace(user_data);
        self
    }

    pub fn write(fd: fs::File, buf: ErasedBox, offset: u64) -> Self {
        SqeInner::Write { fd, buf, offset }.into()
    }

    pub fn read(fd: fs::File, buf: ErasedBox, offset: u64) -> Self {
        SqeInner::Read { fd, buf, offset }.into()
    }

    pub fn open(path: impl AsRef<str>) -> Self {
        SqeInner::Open {
            path: path.as_ref().into(),
        }
        .into()
    }

    pub fn create(path: impl AsRef<str>) -> Self {
        SqeInner::Create {
            path: path.as_ref().into(),
        }
        .into()
    }

    pub fn stat(fd: fs::File) -> Self {
        SqeInner::Stat { fd }.into()
    }

    pub fn fallocate(fd: fs::File, len: u64) -> Self {
        SqeInner::Fallocate { fd, total_len: len }.into()
    }

    pub fn fsync(fd: fs::File) -> Self {
        SqeInner::Fsync { fd }.into()
    }

    pub fn fdatasync(fd: fs::File) -> Self {
        SqeInner::Fdatasync { fd }.into()
    }

    #[allow(unused)]
    pub fn noop() -> Self {
        SqeInner::Noop.into()
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

    pub(super) fn schedule(&mut self, sqe_id: SqeId, executing: &mut VecDeque<Executing>) -> Pending {
        match self {
            SqeInner::Write { buf, offset, .. } => {
                let buf_len = buf.as_bytes().len();
                let first_sector = (*offset / SECTOR_SIZE as u64) as usize;
                let page_count = buf_len / SECTOR_SIZE;

                executing.extend((0..page_count).map(|page| Executing {
                    sqe: sqe_id,
                    inner: Operation::WriteSector(WriteSector {
                        page_offset: first_sector + page,
                        buf_offset: page * SECTOR_SIZE,
                    }),
                }));
                Pending::ReadWrite {
                    results: Results::new(page_count),
                }
            }
            SqeInner::Read { buf, offset, .. } => {
                let buf_len = buf.as_bytes().len();
                let first_sector = (*offset / SECTOR_SIZE as u64) as usize;
                let page_count = buf_len / SECTOR_SIZE;

                executing.extend((0..page_count).map(|page| Executing {
                    sqe: sqe_id,
                    inner: Operation::ReadSector(ReadSector {
                        page_offset: first_sector + page,
                        buf_offset: page * SECTOR_SIZE,
                    }),
                }));
                Pending::ReadWrite {
                    results: Results::new(page_count),
                }
            }
            SqeInner::Open { .. } => {
                executing.push_back(Executing {
                    sqe: sqe_id,
                    inner: Operation::Open,
                });
                Pending::OneOff
            }
            SqeInner::Create { .. } => {
                executing.push_back(Executing {
                    sqe: sqe_id,
                    inner: Operation::Create,
                });
                Pending::OneOff
            }
            SqeInner::Stat { .. } => {
                executing.push_back(Executing {
                    sqe: sqe_id,
                    inner: Operation::Stat,
                });
                Pending::OneOff
            }
            SqeInner::Fallocate { .. } => {
                executing.push_back(Executing {
                    sqe: sqe_id,
                    inner: Operation::Fallocate,
                });
                Pending::OneOff
            }
            SqeInner::Fsync { fd } => {
                let sector_count = fd.len() / SECTOR_SIZE as u64;
                executing.extend(
                    (0..sector_count)
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
                        }]),
                );
                Pending::Sync {
                    results: Results::new(1 + sector_count as usize),
                }
            }
            SqeInner::Fdatasync { fd } => {
                let sector_count = fd.len() / SECTOR_SIZE as u64;
                executing.extend(
                    (0..sector_count)
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
                        }]),
                );
                Pending::Sync {
                    results: Results::new(1 + sector_count as usize),
                }
            }
            SqeInner::Noop => {
                executing.push_back(Executing {
                    sqe: sqe_id,
                    inner: Operation::Noop,
                });
                Pending::OneOff
            }
        }
    }
}
