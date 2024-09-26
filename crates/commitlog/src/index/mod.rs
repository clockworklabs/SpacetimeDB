use std::io;

use thiserror::Error;
mod indexfile;

pub use indexfile::create_index_file;
pub use indexfile::delete_index_file;
pub use indexfile::offset_index_file_path;
pub use indexfile::IndexFileMut;

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
