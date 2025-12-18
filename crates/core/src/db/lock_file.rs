use std::{fmt, fs::File, path::Path, sync::Arc};

use fs2::FileExt as _;
use spacetimedb_paths::server::ReplicaDir;

use crate::error::{DBError, DatabaseError};

#[derive(Clone)]
pub struct LockFile {
    path: Arc<Path>,
    #[allow(unused)]
    lock: Arc<File>,
}

impl LockFile {
    pub fn lock(root: &ReplicaDir) -> Result<Self, DBError> {
        root.create()?;
        let path = root.0.join("db.lock");
        let lock = File::create(&path)?;
        lock.try_lock_exclusive()
            .map_err(|e| DatabaseError::DatabasedOpened(root.0.clone(), e.into()))?;

        Ok(Self {
            path: path.into(),
            lock: lock.into(),
        })
    }
}

impl fmt::Debug for LockFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockFile").field("path", &self.path).finish()
    }
}
