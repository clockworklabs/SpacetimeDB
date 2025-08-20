//! Utilities for saving and re-using credentials.
//!
//! Users are encouraged to import this module by name and refer to its contents by qualified path, like:
//! ```ignore
//! use spacetimedb_sdk::credentials;
//! fn credential_store() -> credentials::File {
//!     credentials::File::new("my_app")
//! }
//! ```

use home::home_dir;
use spacetimedb_lib::{bsatn, de::Deserialize, ser::Serialize};
use std::path::PathBuf;
use thiserror::Error;

const CREDENTIALS_DIR: &str = ".spacetimedb_client_credentials";

#[derive(Error, Debug)]
pub enum CredentialFileError {
    #[error("Failed to determine user home directory as root for credentials storage")]
    DetermineHomeDir,
    #[error("Error creating credential storage directory {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Error serializing credentials for storage in file")]
    Serialize {
        #[source]
        source: bsatn::EncodeError,
    },
    #[error("Error writing BSATN-serialized credentials to file {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Error reading BSATN-serialized credentials from file {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Error deserializing credentials from bytes stored in file {path}")]
    Deserialize {
        path: PathBuf,
        #[source]
        source: bsatn::DecodeError,
    },
}

/// A file on disk which stores, or can store, a JWT for authenticating with SpacetimeDB.
///
/// The file does not necessarily exist or store credentials.
/// If the credentials have been stored previously, they can be accessed with [`File::load`].
/// New credentials can be saved to disk with [`File::save`].
pub struct File {
    filename: String,
}

#[derive(Serialize, Deserialize)]
struct Credentials {
    token: String,
}

impl File {
    /// Get a handle on a file which stores a SpacetimeDB [`Identity`] and its private access token.
    ///
    /// This method does not create the file or check that it exists.
    ///
    /// Distinct applications running as the same user on the same machine
    /// may share [`Identity`]/token pairs by supplying the same `key`.
    /// Users who desire distinct credentials for their application
    /// should supply a unique `key` per application.
    ///
    /// No additional namespacing is provided to tie the stored token
    /// to a particular SpacetimeDB instance or cluster.
    /// Users who intend to connect to multiple instances or clusters
    /// should use a distinct `key` per cluster.
    pub fn new(key: impl Into<String>) -> Self {
        Self { filename: key.into() }
    }

    fn determine_home_dir() -> Result<PathBuf, CredentialFileError> {
        home_dir().ok_or(CredentialFileError::DetermineHomeDir)
    }

    fn ensure_credentials_dir() -> Result<(), CredentialFileError> {
        let mut path = Self::determine_home_dir()?;
        path.push(CREDENTIALS_DIR);

        std::fs::create_dir_all(&path).map_err(|source| CredentialFileError::CreateDir { path, source })
    }

    fn path(&self) -> Result<PathBuf, CredentialFileError> {
        let mut path = Self::determine_home_dir()?;
        path.push(CREDENTIALS_DIR);
        path.push(&self.filename);
        Ok(path)
    }

    /// Store the provided `token` to disk in the file referred to by `self`.
    ///
    /// Future calls to [`Self::load`] on a `File` with the same key can retrieve the token.
    ///
    /// Expected usage is to call this from a [`super::DbConnectionBuilder::on_connect`] callback.
    ///
    /// ```ignore
    /// DbConnection::builder()
    ///   .on_connect(|_ctx, _identity, token| {
    ///       credentials::File::new("my_app").save(token).unwrap();
    /// })
    /// ```
    pub fn save(self, token: impl Into<String>) -> Result<(), CredentialFileError> {
        Self::ensure_credentials_dir()?;

        let creds = bsatn::to_vec(&Credentials { token: token.into() })
            .map_err(|source| CredentialFileError::Serialize { source })?;
        let path = self.path()?;
        std::fs::write(&path, creds).map_err(|source| CredentialFileError::Write { path, source })?;
        Ok(())
    }

    /// Load a saved token from disk in the file referred to by `self`,
    /// if they have previously been stored by [`Self::save`].
    ///
    /// Returns `Err` if I/O fails,
    /// `None` if credentials have not previously been stored,
    /// or `Some` if credentials are successfully loaded from disk.
    /// After unwrapping the `Result`, the returned `Option` can be passed to
    /// [`super::DbConnectionBuilder::with_token`].
    ///
    /// ```ignore
    /// DbConnection::builder()
    ///   .with_token(credentials::File::new("my_app").load().unwrap())
    /// ```
    pub fn load(self) -> Result<Option<String>, CredentialFileError> {
        let path = self.path()?;

        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => return Ok(None),
            Err(source) => return Err(CredentialFileError::Read { path, source }),
        };

        let creds = bsatn::from_slice::<Credentials>(&bytes)
            .map_err(|source| CredentialFileError::Deserialize { path, source })?;
        Ok(Some(creds.token))
    }
}
