use std::{
    fs,
    io::{self, Write as _},
};

use spacetimedb_fs_utils::dir_trie::DirTrie;
use spacetimedb_paths::server::TablesDir;
use spacetimedb_snapshot::FileOrDirPath;
use spacetimedb_table::{
    blob_store::BlobHash,
    page_pool::PagePool,
    tiered::{Object, ObjectBytes, ObjectKey, ObjectMetadata, ObjectStore, ObjectStoreError, RemoveObject},
};

#[derive(Debug)]
pub struct DiskStorage {
    dir_trie: DirTrie,
    page_pool: PagePool,
}

impl DiskStorage {
    pub fn open(dir: TablesDir, page_pool: PagePool) -> io::Result<Self> {
        if let Err(e) = fs::remove_dir_all(&dir)
            && e.kind() != io::ErrorKind::NotFound
        {
            return Err(e);
        }
        fs::create_dir_all(&dir)?;
        let dir_trie = DirTrie::open(dir.0)?;

        Ok(Self { dir_trie, page_pool })
    }

    fn write_trie_entry(&self, hash: &[u8; ObjectKey::SIZE], bytes: &[u8]) -> io::Result<()> {
        let path = self.dir_trie.file_path(hash);
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
}

impl ObjectStore for DiskStorage {
    fn stat(&self, key: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
        let path = self.dir_trie.file_path(key.as_bytes());
        path.metadata()
            .map(|meta| ObjectMetadata { size: meta.len() })
            .map_or_else(none_if_not_found, ok_some)
    }

    fn read(&self, key: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
        self.dir_trie
            .read_entry(key.as_bytes())
            .map(Into::into)
            .map_or_else(none_if_not_found, |bytes| {
                self.verify_hash(key, bytes).and_then(ok_some)
            })
    }

    fn write(&self, object: Object) -> Result<ObjectKey, ObjectStoreError> {
        let (key, bytes) = object.into_kv();
        self.write_trie_entry(key.as_bytes(), &bytes)?;
        Ok(key)
    }
}

unsafe impl RemoveObject for DiskStorage {
    unsafe fn remove(&self, key: ObjectKey) -> Result<(), ObjectStoreError> {
        let path = self.dir_trie.file_path(key.as_bytes());
        fs::remove_file(path).or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e.into())
            }
        })
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
