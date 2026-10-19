use std::{
    fs,
    io::{self, Write as _},
};

use spacetimedb_fs_utils::dir_trie::DirTrie;
use spacetimedb_paths::server::TablesDir;
use spacetimedb_snapshot::FileOrDirPath;
use spacetimedb_table::tiered::{Object, ObjectBytes, ObjectKey, ObjectMetadata, ObjectStore, ObjectStoreError};

#[derive(Debug)]
pub struct DiskStorage {
    dir_trie: DirTrie,
}

impl DiskStorage {
    pub fn open(dir: TablesDir) -> io::Result<Self> {
        fs::remove_dir_all(&dir)?;
        fs::create_dir_all(&dir)?;
        let dir_trie = DirTrie::open(dir.0)?;

        Ok(Self { dir_trie })
    }

    fn write_trie_entry(&self, hash: &[u8; ObjectKey::SIZE], bytes: &[u8]) -> io::Result<()> {
        let mut writer = self.dir_trie.open_entry_writer(hash)?;
        writer.write_all(bytes)?;
        writer.flush()?;

        let path = self.dir_trie.file_path(hash);
        FileOrDirPath::File(&path).sync_all()?;
        FileOrDirPath::Dir(path.parent().expect("dir trie files have a parent")).sync_all()?;

        Ok(())
    }
}

impl ObjectStore for DiskStorage {
    fn stat(&self, key: ObjectKey) -> Result<Option<ObjectMetadata>, ObjectStoreError> {
        let path = self.dir_trie.file_path(key.as_bytes());
        path.metadata()
            .map(|meta| ObjectMetadata { size: meta.len() })
            .map_or_else(none_if_not_found, ok_some)
            .map_err(Into::into)
    }

    fn read(&self, key: ObjectKey) -> Result<Option<ObjectBytes>, ObjectStoreError> {
        self.dir_trie
            .read_entry(key.as_bytes())
            .map(Into::into)
            .map_or_else(none_if_not_found, ok_some)
            .map_err(Into::into)
    }

    fn write(&self, object: Object<'_>) -> Result<ObjectKey, ObjectStoreError> {
        match object {
            Object::Page(page_handle) => {
                let (hash, bytes) = page_handle.serialize_for_storage();
                self.write_trie_entry(hash.as_bytes(), &bytes)?;
                Ok(ObjectKey::Page(hash))
            }
            Object::Blob(blob_handle) => {
                let hash = blob_handle.hash();
                self.write_trie_entry(hash.as_bytes(), blob_handle)?;
                Ok(ObjectKey::Blob(hash))
            }
        }
    }
}

fn none_if_not_found<T>(e: io::Error) -> io::Result<Option<T>> {
    if e.kind() == io::ErrorKind::NotFound {
        Ok(None)
    } else {
        Err(e)
    }
}

fn ok_some<T>(x: T) -> io::Result<Option<T>> {
    Ok(Some(x))
}
