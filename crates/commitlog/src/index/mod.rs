
use std::io;

use thiserror::Error;
mod indexfile;

pub use indexfile::create_index;
pub use indexfile::delete_index;
pub use indexfile::IndexFileMut;

pub trait IndexRead<Key: Into<u64> + From<u64>> {
    /// Returns the key and value that is lesser than or equal to the given key
    fn key_lookup(&self, key: Key) -> Result<(Key, u64), IndexError>;
}

/// Trait for writing operations on an index file
pub trait IndexWrite<Key: Into<u64> + From<u64>> {
    /// Appends a new key-value pair to the index file
    fn append(&mut self, key: Key, value: u64) -> Result<(), IndexError>;

    /// Asynchronously flushes any pending changes to the index file
    fn async_flush(&self) -> Result<(), IndexError>;

    /// Truncates the index file up to the specified key, removing all entries after it
    fn truncate(&mut self, key: Key) -> Result<(), IndexError>;
}



#[derive(Error, Debug)]
pub enum IndexError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Index file out of memory")]
    OutOfMemory,

    #[error("Asked key is smaller than the first entry in the index")]
    KeyNotFound,

    #[error("Invalid input: Key should be monotnously increasing")]
    InvalidInput,

    #[error("index file is not readable")]
    InvalidFormat,
}
