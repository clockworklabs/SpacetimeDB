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
    pub fn for_config(config_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        // Ensure the directory exists before attempting to create the lockfile.
        let parent = config_path.as_ref().parent().unwrap();
        if parent != Path::new("") {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Unable to create directory {parent:?} to build lockfile for {:?}",
                    config_path.as_ref(),
                )
            })?;
        }

        let mut path = config_path.as_ref().to_path_buf();
        path.set_extension("lock");
        // Open with `create_new`, which fails if the file already exists.
        std::fs::File::create_new(&path).with_context(|| {
            "Unable to acquire lock on config file {config_path:?}: failed to create lockfile {path:?}"
        })?;

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
