use super::create_parent_dir;
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
    path: PathBuf,
}

impl Lockfile {
    /// Acquire an exclusive lock on the configuration file `config_path`.
    ///
    /// `config_path` should be the full name of the SpacetimeDB configuration file.
    pub fn for_file(file_path: &Path) -> anyhow::Result<Self> {
        // Ensure the directory exists before attempting to create the lockfile.
        create_parent_dir(file_path)?;

        let mut path = file_path.to_path_buf();
        path.set_extension("lock");
        // Open with `create_new`, which fails if the file already exists.
        std::fs::File::create_new(&path)
            .with_context(|| "Unable to acquire lock on file {file_path:?}: failed to create lockfile {path:?}")?;

        Ok(Lockfile { path })
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path)
            .with_context(|| format!("Unable to remove lockfile {:?}", self.path))
            .unwrap();
    }
}
