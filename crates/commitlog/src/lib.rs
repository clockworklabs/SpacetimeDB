#![allow(unused)]

use std::num::NonZeroU16;

mod commit;
mod commitlog;
mod repo;
mod segment;

pub use crate::{
    commit::Commit,
    payload::{Decoder, Encode},
    segment::{Transaction, DEFAULT_LOG_FORMAT_VERSION},
};
pub mod error;
pub mod payload;

#[cfg(test)]
mod tests;

/// [`Commitlog`] options.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Set the log format version to write, and the maximum supported version.
    ///
    /// Choosing a payload format `T` of [`Commitlog`] should usually result in
    /// updating the [`DEFAULT_LOG_FORMAT_VERSION`] of this crate. Sometimes it
    /// may however be useful to set the version at runtime, e.g. to experiment
    /// with new or very old versions.
    ///
    /// Default: [`DEFAULT_LOG_FORMAT_VERSION`]
    pub log_format_version: u8,
    /// The maximum size in bytes to which log segments should be allowed to
    /// grow.
    ///
    /// Default: 1GiB
    pub max_segment_size: u64,
    /// The maximum number of records in a commit.
    ///
    /// If this number is exceeded, the commit is flushed to disk even without
    /// explicitly calling [`Commitlog::flush`].
    ///
    /// Default: 65,535
    pub max_records_in_commit: NonZeroU16,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            log_format_version: DEFAULT_LOG_FORMAT_VERSION,
            max_segment_size: 1024 * 1024 * 1024,
            max_records_in_commit: NonZeroU16::MAX,
        }
    }
}
