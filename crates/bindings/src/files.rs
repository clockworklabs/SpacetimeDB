//! File Storage: immutable files whose bytes the database keeps outside its tables.
//!
//! A [`FileRef`] is a read capability. Storing one in a table works like storing the file's bytes:
//! any client that can read the row can download the file. Store a [`FileId`] instead where a row
//! is visible to more people than the file should be.
//!
//! Clients usually upload files over HTTP, outside of any reducer, and the module's `allow_upload`
//! lifecycle reducer decides which uploads to accept:
//!
//! ```ignore
//! #[spacetimedb::reducer(allow_upload)]
//! fn allow_upload(ctx: &ReducerContext, upload: UploadRequest) -> Result<(), String> {
//!     // As in any reducer, the sender is the caller: here, the uploader.
//!     ctx.db.user().identity().find(ctx.sender()).ok_or("only registered users may upload")?;
//!     Ok(())
//! }
//! ```
//!
//! Reducers can also create small files directly with [`Files::create`].
//!
//! The files API takes [`FileId`]s. A [`FileRef`] that a client sends may carry any secret, so a
//! reducer checks it with [`Files::id`] before acting on it. Inside the module, the module is the
//! authority: possessing a valid reference is not, by itself, permission to copy or delete a file.

use crate::rt::read_bytes_source_into;
use crate::{sys, AnonymousViewContext, Identity, IterBuf, ReducerContext, ViewContext};
use bytes::Bytes;
use core::fmt;

pub use spacetimedb_lib::file::content_hash;
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
    /// The content type can't be served as an HTTP header.
    InvalidContentType,
    /// The host reported another error.
    Host(sys::Errno),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchFile => f.write_str("no such file"),
            Self::TooLarge => f.write_str("the file is too large to create in a transaction"),
            Self::Unavailable => f.write_str("this database doesn't support File Storage"),
            Self::InvalidContentType => f.write_str("the content type can't be served as an HTTP header"),
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
            sys::Errno::INVALID_CONTENT_TYPE => Self::InvalidContentType,
            errno => Self::Host(errno),
        }
    }
}

fn ref_from_bytes(bytes: [u8; 32]) -> FileRef {
    FileRef::from_u256(spacetimedb_lib::sats::u256::from_le_bytes(bytes))
}

/// The download URL of a file, as a path on the host that serves this database.
///
/// A module doesn't know the address clients reach the host at, so the path starts at `/v1`.
/// Client SDKs build absolute URLs with `conn.files().url(file_ref)`.
pub trait FileUrl {
    fn url(&self) -> String;
}

impl FileUrl for FileRef {
    fn url(&self) -> String {
        let database = Identity::from_byte_array(sys::identity());
        format!("/v1/database/{}/files/{self}", database.to_hex())
    }
}

impl FileUrl for FileMetadata {
    fn url(&self) -> String {
        self.file_ref().url()
    }
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
    /// Creates a file from `bytes`, owned by a new row of `st_file`, and returns its ID.
    ///
    /// Call [`Self::reference`] to get a [`FileRef`] when disclosing the file.
    /// The bytes travel through the commitlog with this transaction, so they are limited to 16 MiB.
    /// Clients that read with confirmed reads see the new file only once its bytes are persisted.
    pub fn create(&self, bytes: impl Into<Bytes>, content_type: Option<String>) -> Result<FileId, FileError> {
        let bytes = bytes.into();
        Ok(FileId(u128::from_le_bytes(sys::file_create(
            &bytes,
            content_type.as_deref(),
        )?)))
    }

    /// The file's ID, if `file_ref`'s secret matches a live file.
    ///
    /// `None` if the reference is forged or the file has been deleted. Use it on references from
    /// clients. A reference the module read from its own tables can use [`FileRef::id_unchecked`].
    pub fn id(&self, file_ref: FileRef) -> Option<FileId> {
        id(file_ref)
    }

    /// The live metadata of the file `id`, without fetching its bytes.
    ///
    /// It contains the file's reference, so authorize the requester before disclosing it.
    pub fn metadata(&self, id: FileId) -> Option<FileMetadata> {
        metadata(id)
    }

    /// Creates a new file that shares `source`'s bytes and content type, and returns its ID.
    ///
    /// The copy has its own ID and read secret, and lives until it is deleted,
    /// whatever happens to `source`.
    pub fn copy(&self, source: FileId) -> Result<FileId, FileError> {
        Ok(FileId(u128::from_le_bytes(sys::file_copy(source.0.to_le_bytes())?)))
    }

    /// Deletes the live file `id`. Returns `false` if there was none.
    ///
    /// Deleting a file doesn't revoke downloads of bytes that history still retains.
    pub fn delete(&self, id: FileId) -> Result<bool, FileError> {
        Ok(sys::file_delete(id.0.to_le_bytes())?)
    }

    /// Resolves a file's ID to its reference.
    ///
    /// The reference grants read access, so check that the requester may read the file
    /// before disclosing it.
    pub fn reference(&self, id: FileId) -> Option<FileRef> {
        reference(id)
    }
}

/// Read-only File Storage for views, from [`ViewContext::files`] and [`AnonymousViewContext::files`].
///
/// A view that reads a file's metadata is re-evaluated when files change.
#[derive(Clone, Copy)]
pub struct ReadonlyFiles<'ctx> {
    _ctx: core::marker::PhantomData<&'ctx ()>,
}

impl ViewContext {
    /// Read-only File Storage.
    pub fn files(&self) -> ReadonlyFiles<'_> {
        ReadonlyFiles {
            _ctx: core::marker::PhantomData,
        }
    }
}

impl AnonymousViewContext {
    /// Read-only File Storage.
    pub fn files(&self) -> ReadonlyFiles<'_> {
        ReadonlyFiles {
            _ctx: core::marker::PhantomData,
        }
    }
}

impl ReadonlyFiles<'_> {
    /// The file's ID, if `file_ref`'s secret matches a live file. See [`Files::id`].
    pub fn id(&self, file_ref: FileRef) -> Option<FileId> {
        id(file_ref)
    }

    /// The live metadata of the file `id`, without fetching its bytes.
    ///
    /// It contains the file's reference, so authorize the requester before disclosing it.
    pub fn metadata(&self, id: FileId) -> Option<FileMetadata> {
        metadata(id)
    }

    /// Resolves a file's ID to its reference.
    ///
    /// The reference grants read access, so check that the requester may read the file
    /// before disclosing it.
    pub fn reference(&self, id: FileId) -> Option<FileRef> {
        reference(id)
    }
}

fn id(file_ref: FileRef) -> Option<FileId> {
    let id = file_ref.id_unchecked();
    (reference(id) == Some(file_ref)).then_some(id)
}

fn metadata(id: FileId) -> Option<FileMetadata> {
    let source = sys::file_metadata(id.0.to_le_bytes())
        .unwrap_or_else(|errno| panic!("failed to read file metadata: {errno}"))?;
    let mut buf = IterBuf::take();
    read_bytes_source_into(source, &mut buf);
    Some(spacetimedb_lib::bsatn::from_slice(&buf).expect("the host sends valid file metadata"))
}

fn reference(id: FileId) -> Option<FileRef> {
    match sys::file_reference(id.0.to_le_bytes()) {
        Ok(bytes) => Some(ref_from_bytes(bytes)),
        Err(sys::Errno::NO_SUCH_FILE) => None,
        Err(errno) => panic!("failed to resolve a file ID: {errno}"),
    }
}
