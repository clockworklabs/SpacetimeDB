//! File Storage: immutable files whose bytes the database keeps outside its tables.
//!
//! A [`FileRef`] is a read capability. Storing one in a table works like storing the file's bytes:
//! any client that can read the row can download the file. Store a [`FileId`] instead where a row
//! is visible to more people than the file should be.
//!
//! Clients usually upload files over HTTP, outside of any reducer, and the module's `allow_upload`
//! reducer decides which uploads to accept:
//!
//! ```ignore
//! #[spacetimedb::reducer]
//! fn allow_upload(ctx: &ReducerContext, upload: UploadRequest) -> Result<(), String> {
//!     upload.sender.ok_or("sign in to upload")?;
//!     Ok(())
//! }
//! ```
//!
//! Reducers can also create small files directly with [`Files::create`].

use crate::rt::read_bytes_source_into;
use crate::{sys, IterBuf, ReducerContext};
use core::fmt;

pub use spacetimedb_lib::file::{content_hash, ALLOW_UPLOAD_REDUCER};
pub use spacetimedb_lib::{FileId, FileMetadata, FileRef, UploadRequest};

/// Why a file operation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileError {
    /// No live file matches the reference.
    NoSuchFile,
    /// The file is too large to create in a transaction. Upload it instead.
    TooLarge,
    /// This database doesn't support File Storage.
    Unavailable,
    /// The host reported another error.
    Host(sys::Errno),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchFile => f.write_str("no such file"),
            Self::TooLarge => f.write_str("the file is too large to create in a transaction"),
            Self::Unavailable => f.write_str("this database doesn't support File Storage"),
            Self::Host(errno) => write!(f, "file operation failed: {errno}"),
        }
    }
}

impl std::error::Error for FileError {}

impl From<sys::Errno> for FileError {
    fn from(errno: sys::Errno) -> Self {
        match errno {
            sys::Errno::NO_SUCH_FILE => Self::NoSuchFile,
            sys::Errno::FILE_TOO_LARGE => Self::TooLarge,
            sys::Errno::FILES_UNAVAILABLE => Self::Unavailable,
            errno => Self::Host(errno),
        }
    }
}

fn to_bytes(file_ref: FileRef) -> [u8; 32] {
    file_ref.to_u256().to_le_bytes()
}

fn from_bytes(bytes: [u8; 32]) -> FileRef {
    FileRef::from_u256(spacetimedb_lib::sats::u256::from_le_bytes(bytes))
}

/// File Storage within the current transaction, from [`ReducerContext::files`].
#[derive(Clone, Copy)]
pub struct Files<'ctx> {
    _ctx: &'ctx ReducerContext,
}

impl ReducerContext {
    /// File Storage within this transaction.
    pub fn files(&self) -> Files<'_> {
        Files { _ctx: self }
    }
}

impl Files<'_> {
    /// Creates a file from `bytes`, owned by a new row of `st_file`.
    ///
    /// The bytes travel through the commitlog with this transaction, so they are limited to 16 MiB.
    /// Clients that read with confirmed reads see the new file only once its bytes are persisted.
    pub fn create(&self, bytes: impl AsRef<[u8]>, content_type: Option<&str>) -> Result<FileRef, FileError> {
        Ok(from_bytes(sys::file_create(bytes.as_ref(), content_type)?))
    }

    /// Creates a new file that shares `source`'s bytes and content type.
    ///
    /// The copy has its own ID and read secret, and lives until it is deleted,
    /// whatever happens to `source`.
    pub fn copy(&self, source: FileRef) -> Result<FileRef, FileError> {
        Ok(from_bytes(sys::file_copy(to_bytes(source))?))
    }

    /// Deletes the file `file_ref` names. Returns `false` if there was none.
    ///
    /// Deleting a file doesn't revoke downloads of bytes that history still retains.
    pub fn delete(&self, file_ref: FileRef) -> Result<bool, FileError> {
        Ok(sys::file_delete(to_bytes(file_ref))?)
    }

    /// The live metadata of the file `file_ref` names, without fetching its bytes.
    pub fn metadata(&self, file_ref: FileRef) -> Option<FileMetadata> {
        let source = sys::file_metadata(to_bytes(file_ref))
            .unwrap_or_else(|errno| panic!("failed to read file metadata: {errno}"))?;
        let mut buf = IterBuf::take();
        read_bytes_source_into(source, &mut buf);
        Some(spacetimedb_lib::bsatn::from_slice(&buf).expect("the host sends valid file metadata"))
    }

    /// Resolves a file's ID to its reference.
    ///
    /// The reference grants read access, so check that the requester may read the file
    /// before disclosing it.
    pub fn reference(&self, id: FileId) -> Option<FileRef> {
        match sys::file_reference(id.0.to_le_bytes()) {
            Ok(bytes) => Some(from_bytes(bytes)),
            Err(sys::Errno::NO_SUCH_FILE) => None,
            Err(errno) => panic!("failed to resolve a file ID: {errno}"),
        }
    }
}
