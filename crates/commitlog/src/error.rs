use std::io;

use spacetimedb_sats::buffer::DecodeError;
use thiserror::Error;

use crate::segment;

/// Error yielded by public commitlog iterators.
#[derive(Debug, Error)]
pub enum Traversal {
    #[error("out-of-order commit: expected-offset={expected_offset} actual-offset={actual_offset}")]
    OutOfOrder {
        expected_offset: u64,
        actual_offset: u64,
        /// If the next segment starts with a commit with matching offset, a
        /// previous bad commit will be ignored. If, however, the offset does
        /// **not** match, `prev_error` contains the error encountered when
        /// trying to read the previous commit (which was skipped).
        #[source]
        prev_error: Option<Box<Self>>,
    },
    /// The log is considered forked iff a commit with the same `min_tx_offset`
    /// but a different crc32 than the previous commit is encountered.
    ///
    /// This may happen in rare circumstances where a write was considered
    /// failed (e.g. due to a failed `fsync(2)`), when it was actually successful.
    #[error("forked history: offset={offset}")]
    Forked { offset: u64 },
    #[error("failed to decode tx record at offset={offset}")]
    Decode {
        offset: u64,
        #[source]
        source: DecodeError,
    },
    #[error("checksum mismatch at offset={offset}")]
    Checksum {
        offset: u64,
        #[source]
        source: ChecksumMismatch,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Error returned by [`crate::Commitlog::append`].
#[derive(Debug, Error)]
#[error("failed to commit during append")]
pub struct Append<T> {
    /// The payload which was passed to [`crate::Commitlog::append`], but was
    /// not retained because flushing the data to the underlying storage failed.
    pub txdata: T,
    /// Why flushing to persistent storage failed.
    #[source]
    pub source: io::Error,
}

/// A checksum mismatch was detected.
///
/// Usually wrapped in another error, such as [`io::Error`].
#[derive(Debug, Error)]
#[error("checksum mismatch")]
pub struct ChecksumMismatch;

#[derive(Debug, Error)]
pub enum SegmentMetadata {
    #[error("invalid commit encountered")]
    InvalidCommit {
        sofar: segment::Metadata,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
}
