use crate::create_parent_dir;
use std::path::{Path, PathBuf};

#[derive(thiserror::Error, Debug)]
pub enum LockfileError {
    #[error("Failed to acquire lock on {file_path:?}: failed to create lockfile {lock_path:?}: {cause}")]
    Acquire {
        file_path: PathBuf,
        lock_path: PathBuf,
        #[source]
        cause: std::io::Error,
    },
    #[error("Failed to release lock: failed to delete lockfile {lock_path:?}: {cause}")]
    Release {
        lock_path: PathBuf,
        #[source]
        cause: std::io::Error,
    },
}

#[derive(Debug)]
/// A file used as an exclusive lock on access to another file.
///
/// Constructing a `Lockfile` creates the `path` with [`std::fs::File::create_new`],
/// a.k.a. `O_EXCL`, erroring if the file already exists.
///
/// Dropping a `Lockfile` deletes the `path`, releasing the lock.
///
/// Used to guarantee exclusive access to the system config file,
/// in order to prevent racy concurrent modifications.
pub struct Lockfile {
    path: PathBuf,
}

impl Lockfile {
    /// Acquire an exclusive lock on the file `file_path`.
    ///
    /// `file_path` should be the full path of the file to which to acquire exclusive access.
    pub fn for_file<P: AsRef<Path>>(file_path: P) -> Result<Self, LockfileError> {
        let file_path = file_path.as_ref();
        // TODO: Someday, it would be nice to use OS locks to minimize edge cases (see
        // https://github.com/clockworklabs/SpacetimeDB/pull/1341#issuecomment-2151018992).
        //
        // Currently, our files can be left around if a process is unceremoniously killed (most
        // commonly with Ctrl-C, but this would also apply to e.g. power failure).
        // See https://github.com/clockworklabs/SpacetimeDB/issues/1339.
        let path = Self::lock_path(file_path);

        let fail = |cause| LockfileError::Acquire {
            lock_path: path.clone(),
            file_path: file_path.to_path_buf(),
            cause,
        };
        // Ensure the directory exists before attempting to create the lockfile.
        create_parent_dir(file_path).map_err(fail)?;
        // Open with `create_new`, which fails if the file already exists.
        std::fs::File::create_new(&path).map_err(fail)?;
        Ok(Lockfile { path })
    }

    /// Returns the path of a lockfile for the file `file_path`,
    /// without actually acquiring the lock.
    pub fn lock_path<P: AsRef<Path>>(file_path: P) -> PathBuf {
        file_path.as_ref().with_extension("lock")
    }

    fn release_internal(path: &Path) -> Result<(), LockfileError> {
        std::fs::remove_file(path).map_err(|cause| LockfileError::Release {
            lock_path: path.to_path_buf(),
            cause,
        })
    }

    /// Release the lock, with the opportunity to handle the error from failing to delete the lockfile.
    ///
    /// Dropping a [`Lockfile`] will release the lock, but will panic on failure rather than returning `Err`.
    pub fn release(self) -> Result<(), LockfileError> {
        // Don't run the destructor, which does the same thing, but panics on failure.
        let mut this = std::mem::ManuallyDrop::new(self);
        let path = std::mem::take(&mut this.path);
        let res = Self::release_internal(&path);
        drop(path);
        res
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        Self::release_internal(&self.path).unwrap();
    }
}

pub mod advisory {
    use chrono::{DateTime, Utc};
    use std::{
        error::Error as StdError,
        fmt,
        fs::File,
        io::{self, Read, Seek, SeekFrom, Write},
        path::{Path, PathBuf},
        process,
        time::SystemTime,
    };

    use crate::create_parent_dir;
    use fs2::FileExt as _;

    #[derive(Debug)]
    pub struct LockError {
        pub path: PathBuf,
        pub source: io::Error,
        pub existing_contents: Option<String>,
    }

    impl fmt::Display for LockError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "failed to lock {}", self.path.display())?;
            if let Some(contents) = &self.existing_contents {
                write!(f, " (existing contents: {:?})", contents)?;
            }
            Ok(())
        }
    }

    impl StdError for LockError {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(&self.source)
        }
    }

    /// A file locked with an exclusive, filesystem-level lock.
    ///
    /// Uses [`flock(2)`] on Unix platforms, and [`LockFile`] on Windows systems.
    ///
    /// The file is created if it does not exist.
    /// Dropping `Lockfile` releases the lock, but, unlike [super::Lockfile],
    /// does not delete the file.
    ///
    /// [`flock(2)`]: https://man7.org/linux/man-pages/man2/flock.2.html
    /// [`LockFile`]: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfile?redirectedfrom=MSDN
    pub struct LockedFile {
        path: PathBuf,
        #[allow(unused)]
        lock: File,
    }

    impl LockedFile {
        /// Attempt to lock `path` using an exclusive lock.
        ///
        /// The file will be created,
        /// including its parent directories,
        /// if it does not exist.
        ///
        /// Note that, unlike [super::Lockfile::for_file],
        /// the exact `path` is used -- no extra adjacent `.lock` file is
        /// created.
        pub fn lock(path: impl AsRef<Path>) -> Result<Self, LockError> {
            let path = path.as_ref();
            Self::lock_inner(path)
        }

        /// Replace the lock file contents with `metadata` while holding the lock.
        pub fn write_metadata(&mut self, metadata: impl AsRef<[u8]>) -> io::Result<()> {
            self.lock.set_len(0)?;
            self.lock.seek(SeekFrom::Start(0))?;
            self.lock.write_all(metadata.as_ref())?;
            self.lock.sync_data()?;
            Ok(())
        }

        fn lock_inner(path: &Path) -> Result<Self, LockError> {
            create_parent_dir(path).map_err(|source| LockError {
                path: path.to_path_buf(),
                source,
                existing_contents: None,
            })?;
            // This will create the file if it doesn't already exist.
            let mut lock = File::options()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .map_err(|source| LockError {
                    path: path.to_path_buf(),
                    source,
                    existing_contents: None,
                })?;
            // TODO: Use `File::lock` (available since rust 1.89) instead?
            if let Err(source) = lock.try_lock_exclusive() {
                let existing_contents = if source.kind() == io::ErrorKind::WouldBlock {
                    Self::read_existing_contents(&mut lock).ok().flatten()
                } else {
                    None
                };
                return Err(LockError {
                    path: path.to_path_buf(),
                    source,
                    existing_contents,
                });
            }
            // Now that we own the lock, clear any content that may have been written by a previous holder.
            lock.set_len(0).map_err(|source| LockError {
                path: path.to_path_buf(),
                source,
                existing_contents: None,
            })?;
            lock.seek(SeekFrom::Start(0)).map_err(|source| LockError {
                path: path.to_path_buf(),
                source,
                existing_contents: None,
            })?;

            let mut locked = Self {
                path: path.to_path_buf(),
                lock,
            };
            // Write the default metadata.
            locked
                .write_metadata(Self::default_metadata())
                .map_err(|source| LockError {
                    path: path.to_path_buf(),
                    source,
                    existing_contents: None,
                })?;

            Ok(locked)
        }

        fn read_existing_contents(lock: &mut File) -> io::Result<Option<String>> {
            lock.seek(SeekFrom::Start(0))?;
            let mut bytes = Vec::new();
            lock.read_to_end(&mut bytes)?;
            if bytes.is_empty() {
                return Ok(None);
            }
            Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
        }

        // Default contents of a lockfile, which has the pid and timestamp.
        fn default_metadata() -> String {
            let timestamp_ms = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let timestamp = DateTime::<Utc>::from_timestamp_millis(timestamp_ms).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
            format!("pid={};timestamp_utc={}", process::id(), timestamp.to_rfc3339())
        }
    }

    impl fmt::Debug for LockedFile {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("LockedFile").field("path", &self.path).finish()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::ErrorKind};

    use tempdir::TempDir;

    use super::advisory::LockedFile;

    #[test]
    fn lockedfile_can_create_a_file() {
        let tmp = TempDir::new("lockfile_test").unwrap();
        let path = tmp.path().join("db.lock");
        let _lock1 = LockedFile::lock(&path).unwrap();
        assert!(path.exists());
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains(&format!("pid={}", std::process::id())));
        assert!(contents.contains("timestamp_utc="));
    }

    #[test]
    fn lockedfile_can_create_a_directory_file() {
        let tmp = TempDir::new("lockfile_test").unwrap();
        let path = tmp.path().join("new_dir").join("db.lock");
        let _lock1 = LockedFile::lock(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn only_one_exclusive_lock_can_be_held() {
        let tmp = TempDir::new("lockfile_test").unwrap();
        let path = tmp.path().join("db.lock");
        let _lock1 = LockedFile::lock(&path).unwrap();

        assert!(LockedFile::lock(&path).is_err());
    }

    #[test]
    fn lockedfile_can_handle_existing_file() {
        let tmp = TempDir::new("locked_file_test").unwrap();
        let path = tmp.path().join("db.lock");
        let original = b"existing lock metadata";
        fs::write(&path, original).unwrap();

        let _lock = LockedFile::lock(&path).unwrap();

        // Previous metadata should be replaced when we acquire the lock.
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains(&format!("pid={}", std::process::id())));
        assert!(contents.contains("timestamp_utc="));
    }

    #[test]
    fn lockedfile_can_store_metadata() {
        let tmp = TempDir::new("locked_file_test").unwrap();
        let path = tmp.path().join("db.lock");
        let mut lock = LockedFile::lock(&path).unwrap();

        lock.write_metadata("pid=1234").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "pid=1234");
    }

    #[test]
    fn lock_error_includes_existing_contents_when_already_locked() {
        let tmp = TempDir::new("locked_file_test").unwrap();
        let path = tmp.path().join("db.lock");
        let mut lock = LockedFile::lock(&path).unwrap();
        lock.write_metadata("pid=1234").unwrap();

        let err = LockedFile::lock(&path).unwrap_err();
        assert_eq!(err.source.kind(), ErrorKind::WouldBlock);
        assert_eq!(err.existing_contents.as_deref(), Some("pid=1234"));
        assert!(err.to_string().contains("pid=1234"));
    }
}
