use anyhow::Context;
use std::path::{Path, PathBuf};

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
    lock_path: PathBuf,
}

impl Lockfile {
    /// Acquire an exclusive lock on the file `lock_path`.
    pub fn acquire(lock_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        // Ensure the directory exists before attempting to create the lockfile.
        let parent = lock_path.as_ref().parent().unwrap();
        if parent != Path::new("") {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Unable to create directory {parent:?} to build lockfile for {:?}",
                    lock_path.as_ref(),
                )
            })?;
        }

        // Open with `create_new`, which fails if the file already exists.
        std::fs::File::create_new(&lock_path).with_context(|| "Unable to acquire lockfile {lock_path:?}")?;

        Ok(Lockfile {
            lock_path: lock_path.as_ref().to_path_buf(),
        })
    }

    /// Acquire an exclusive lock on a lockfile corresponding to `path`.
    /// Note that the lockfile will be a path distinct from `path`, so this lock is "only enforced"
    /// for other callers to `Lockfile::for_file`.
    ///
    /// If you want to acquire a lockfile at `path`, see `acquire`.
    pub fn for_path(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut lock_path = path.as_ref().to_path_buf();
        lock_path.set_extension("lock");
        Lockfile::acquire(lock_path)
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.lock_path)
            .with_context(|| format!("Unable to remove lockfile {:?}", self.lock_path))
            .unwrap();
    }
}
