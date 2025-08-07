use std::io;

use thiserror::Error;
mod indexfile;

pub use indexfile::{IndexFile, IndexFileMut};

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Index file out of range")]
    OutOfRange,

    #[error("Asked key is smaller than the first entry in the index")]
    KeyNotFound,

    #[error("Key should be monotonically increasing: input: {1}, last: {0}")]
    InvalidInput(u64, u64),

    #[error("index file is not readable")]
    InvalidFormat,
}
