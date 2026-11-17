use std::{
    fs,
    io::{self, Write as _},
    path::PathBuf,
};

use spacetimedb_fs_utils::dir_trie::DirTrie;
use spacetimedb_paths::server::TablesDir;
use spacetimedb_snapshot::FileOrDirPath;
use spacetimedb_table::tiered::metrics::{DiskOperation, Metrics};
use spacetimedb_table::{
    blob_store::BlobHash,
    page_pool::PagePool,
    tiered::{Object, ObjectBytes, ObjectKey, ObjectMetadata, ObjectStore, ObjectStoreError, RemoveObject},
};

#[derive(Debug)]
pub struct DiskStorage {
    pages: DirTrie,
    blobs: DirTrie,
    page_pool: PagePool,
    metrics: Metrics,
}

impl DiskStorage {
    pub fn open(dir: TablesDir, page_pool: PagePool) -> io::Result<Self> {
        if let Err(e) = fs::remove_dir_all(&dir)
            && e.kind() != io::ErrorKind::NotFound
        {
            return Err(e);
        }
        fs::create_dir_all(&dir)?;
        let pages = DirTrie::open(dir.0.join("pages"))?;
        let blobs = DirTrie::open(dir.0.join("blobs"))?;

        Ok(Self {
            pages,
            blobs,
            page_pool,
            metrics: Default::default(),
        })
    }

    fn io<T>(
        &self,
        operation: DiskOperation,
        f: impl FnOnce() -> Result<(T, u64), ObjectStoreError>,
    ) -> Result<T, ObjectStoreError> {
        let timer = self.metrics.timer();
        let result = f();
        self.metrics
            .disk_io(operation, timer.nanos(), result.as_ref().map_or(0, |(_, bytes)| *bytes));
        result.map(|(value, _)| value)
    }

    fn record_operation<T>(&self, operation: DiskOperation, result: &Result<T, ObjectStoreError>) {
        self.metrics.disk_operation(operation, result.is_ok());
        let Err(error) = result else { return };
        let Some(stats) = self.metrics.collected() else { return };
        log::warn!(
            "database_identity={} replica_id={} operation={}: disk cache operation failed: {error:?}",
            stats.database,
            stats.replica,
            operation.as_str(),
        );
    }

    fn write_trie_entry(&self, key: &ObjectKey, bytes: &[u8]) -> io::Result<()> {
        let path = self.file_path(key);
        let dir = path.parent().expect("dir trie file paths have a parent");
        fs::create_dir_all(dir)?;

        let mut tmp = tempfile::Builder::new().make_in(dir, |tmp_path| {
            fs::File::options()
                .read(true)
                .write(true)
                .create_new(true)
                .open(tmp_path)
        })?;
        tmp.write_all(bytes)?;
        tmp.flush()?;
        tmp.as_file().sync_data()?;
        tmp.persist(&path)?;
        FileOrDirPath::Dir(dir).sync_all()?;

        Ok(())
    }

    fn verify_hash(&self, key: ObjectKey, bytes: ObjectBytes) -> Result<ObjectBytes, ObjectStoreError> {
        let (computed, expected) = match key {
            ObjectKey::Page(expected) => {
                // TODO: We're deserializing the page twice -- once here and
                // again when installing it in memory. We may need a clever
                // representation of what [ObjectStore::read] returns.
                let computed = self.page_pool.take_deserialize_from(&bytes)?.content_hash();
                (computed, expected.into())
            }
            ObjectKey::Blob(expected) => {
                let computed = BlobHash::hash_from_bytes(&bytes).into();
                (computed, expected.into())
            }
        };

        if computed != expected {
            Err(ObjectStoreError::HashMismatch { expected, computed })
        } else {
            Ok(bytes)
        }
    }

    fn file_path(&self, key: &ObjectKey) -> PathBuf {
        match key {
            ObjectKey::Page(hash) => self.pages.file_path(hash.as_bytes()),
            ObjectKey::Blob(hash) => self.blobs.file_path(hash.as_bytes()),
        }
    }
}

impl ObjectStore for DiskStorage {
    fn set_metrics(&mut self, metrics: Metrics) {
        self.metrics = metrics;
    }

    fn stat(&self, key: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
        self.file_path(&key)
            .metadata()
            .map(|meta| ObjectMetadata { size: meta.len() })
            .map_or_else(none_if_not_found, ok_some)
    }

    fn read(&self, key: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
        let result = self.io(DiskOperation::Read, || {
            let raw = match key {
                ObjectKey::Page(hash) => self.pages.read_entry(hash.as_bytes()),
                ObjectKey::Blob(hash) => self.blobs.read_entry(hash.as_bytes()),
            };
            let bytes = raw.map(ObjectBytes::from).map_or_else(none_if_not_found, ok_some)?;
            let len = bytes.as_ref().map_or(0, |bytes| bytes.len() as u64);
            Ok((bytes, len))
        });
        let result = result.and_then(|bytes| bytes.map(|bytes| self.verify_hash(key, bytes)).transpose());
        self.record_operation(DiskOperation::Read, &result);
        result
    }

    fn write(&self, object: Object) -> Result<ObjectKey, ObjectStoreError> {
        let (key, bytes) = object.into_kv();
        let result = self.io(DiskOperation::Write, || {
            self.write_trie_entry(&key, &bytes)?;
            Ok((key, bytes.len() as u64))
        });
        self.record_operation(DiskOperation::Write, &result);
        result
    }
}

unsafe impl RemoveObject for DiskStorage {
    unsafe fn remove(&self, key: ObjectKey) -> Result<(), ObjectStoreError> {
        let path = self.file_path(&key);
        let result = fs::remove_file(path).or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e.into())
            }
        });
        self.record_operation(DiskOperation::Delete, &result);
        result
    }
}

fn none_if_not_found<T>(e: io::Error) -> Result<Option<T>, ObjectStoreError> {
    match e.kind() {
        io::ErrorKind::NotFound => Ok(None),
        _ => Err(e.into()),
    }
}

fn ok_some<T>(x: T) -> Result<Option<T>, ObjectStoreError> {
    Ok(Some(x))
}
