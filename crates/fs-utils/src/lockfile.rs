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
    use std::{
        fmt,
        fs::{self, File},
        io,
        path::{Path, PathBuf},
    };

    use fs2::FileExt as _;
    use thiserror::Error;

    use crate::create_parent_dir;

    #[derive(Debug, Error)]
    #[error("failed to lock {}", path.display())]
    pub struct LockError {
        pub path: PathBuf,
        #[source]
        pub source: io::Error,
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
            Self::lock_inner(path).map_err(|source| LockError {
                path: path.into(),
                source,
            })
        }

        fn lock_inner(path: &Path) -> io::Result<Self> {
            create_parent_dir(path)?;
            let lock = File::create(path)?;
            // TODO: Use `File::lock` (available since rust 1.89) instead?
            lock.try_lock_exclusive()?;

            Ok(Self {
                path: path.to_path_buf(),
                lock,
            })
        }

        /// Release the lock and optionally remove the locked file.
        pub fn release(self, remove: bool) -> io::Result<()> {
            if remove {
                fs::remove_file(&self.path)?;
            }
            Ok(())
        }
    }

    impl fmt::Debug for LockedFile {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("LockedFile").field("path", &self.path).finish()
        }
    }
}
