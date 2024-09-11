use std::io;

use thiserror::Error;
mod indexfile;

pub use indexfile::create_index_file;
pub use indexfile::delete_index_file;
pub use indexfile::offset_index_file_path;
pub use indexfile::IndexFileMut;

pub trait IndexRead<Key: Into<u64> + From<u64>> {
    // Return (key, value) pair of key just smaller or equal to given key
    ///
    /// # Error
    /// - `IndexError::KeyNotFound`: If the key is smaller than the first entry key
    #[allow(dead_code)]
    fn key_lookup(&self, key: Key) -> Result<(Key, u64), IndexError>;
}

/// Trait for writing operations on an index file
pub trait IndexWrite<Key: Into<u64> + From<u64>> {
    /// Appends a key-value pair to the index file.
    /// Successive calls to `append` must supply key in ascending order
    ///
    /// Errors
    /// - `IndexError::InvalidInput`: Either Key or Value is 0
    /// - `IndexError::OutOfMemory`: Append after index file is already full.
    fn append(&mut self, key: Key, value: u64) -> Result<(), IndexError>;

    /// Asynchronously flushes any pending changes to the index file
    fn async_flush(&self) -> Result<(), IndexError>;

    /// Truncates the index file starting from the entry with a key greater than or equal to the given key.
    fn truncate(&mut self, key: Key) -> Result<(), IndexError>;
}

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Index file out of range")]
    OutOfRange,

    #[error("Asked key is smaller than the first entry in the index")]
    KeyNotFound,

    #[error("Key should be monotnously increasing")]
    InvalidInput,

    #[error("index file is not readable")]
    InvalidFormat,
}
