use crate::db::ostorage::ObjectDB;
use crate::hash::{hash_bytes, Hash};
use hex;

use std::{
    collections::HashMap,
    fs::{self, read_dir, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::error::DBError;
#[cfg(target_family = "unix")]
use std::os::unix::prelude::MetadataExt;

pub struct HashMapObjectDB {
    root: PathBuf,
    map: HashMap<Hash, Vec<u8>>,
    obj_size: u64,
    unsynced: Vec<File>,
}

impl HashMapObjectDB {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DBError> {
        let root = path.as_ref();

        Self::create_directories(root)?;

        let mut cache: HashMap<Hash, Vec<u8>> = HashMap::new();
        let mut obj_size: u64 = 0;

        for item in read_dir(root)? {
            let dir_entry = item?;
            let path = dir_entry.path();
            let dir_name = path.file_name().unwrap().to_str().unwrap();
            let hex_dir_name = hex::decode(dir_name);
            if let Err(err) = hex_dir_name {
                log::warn!("invalid object dir found: {} {:?}", dir_name, err);
                continue;
            }
            let hex_dir_bytes = hex_dir_name?;
            if hex_dir_bytes.len() != 1 {
                log::warn!("invalid object dir found, name longer than 1");
                continue;
            }
            let first_byte: u8 = hex_dir_bytes[0];

            let inner_dir = &PathBuf::from(root).join(path);
            for item in read_dir(inner_dir)? {
                let dir_entry = item?;
                #[cfg(target_family = "unix")]
                let size = dir_entry.metadata()?.size();
                #[cfg(target_family = "windows")]
                let size = dir_entry.metadata()?.len();
                let path = dir_entry.path();
                let dir_name = path.file_name().unwrap().to_str().unwrap();
                let hex_dir_name = hex::decode(dir_name);
                if let Err(err) = hex_dir_name {
                    log::warn!("invalid object dir found: {:?}", err);
                    continue;
                }
                let hex_dir_bytes = hex_dir_name?;
                if hex_dir_bytes.len() != 31 {
                    log::warn!("invalid object dir found, name longer than 31");
                    continue;
                }
                let mut bytes: [u8; 32] = [0; 32];
                bytes[0] = first_byte;
                bytes[1..].copy_from_slice(&hex_dir_bytes);

                let mut file = OpenOptions::new().read(true).open(inner_dir.join(path))?;

                let mut contents = Vec::new();
                file.read_to_end(&mut contents)?;

                let hash = Hash::from_slice(&bytes);
                cache.insert(hash, contents);
                obj_size += size;
            }
        }

        Ok(Self {
            root: root.to_path_buf(),
            map: cache,
            obj_size,
            unsynced: Vec::new(),
        })
    }

    pub fn total_key_size_bytes(&self) -> u64 {
        self.map.len() as u64 * 32
    }

    pub fn total_obj_size_bytes(&self) -> u64 {
        self.obj_size
    }

    pub fn total_mem_size_bytes(&self) -> u64 {
        self.total_key_size_bytes() + self.total_obj_size_bytes()
    }

    // Create the root directory for this |HashMapObjectDB|,
    // and all subdirectories within it.
    fn create_directories(root: &Path) -> Result<(), DBError> {
        // We want to "own" the directory, and so any error here is
        // just propagated back. In particular, this includes
        // |io::ErrorKind::AlreadyExists|. We gracefully fail in that case.
        fs::create_dir_all(root)?;

        // Now we create all the subdirectories named "00", ..., "7f", ... "ff".
        for prefix in hex_prefixes() {
            let path = root.join(prefix);
            // If we fail with |AlreadyExists|, assume it's because we were previously
            // running and created those directories. Otherwise, return an error.
            match fs::create_dir(path) {
                Ok(()) => (),
                Err(err) => match err.kind() {
                    std::io::ErrorKind::AlreadyExists => (),
                    _ => return Err(err.into()),
                },
            }
        }

        Ok(())
    }
}

impl ObjectDB for HashMapObjectDB {
    fn add(&mut self, bytes: Vec<u8>) -> Hash {
        let hash = hash_bytes(&bytes);
        if self.map.contains_key(&hash) {
            return hash;
        }

        let folder = hex::encode(&hash.data[0..1]);
        let filename = hex::encode(&hash.data[1..]);
        let path = self.root.join(folder).join(filename);

        let mut unsynced = OpenOptions::new().write(true).create(true).open(path).unwrap();
        unsynced.write_all(&bytes).unwrap();
        self.unsynced.push(unsynced);

        // Currently this is hardcoded to be something a bit bigger than one, but
        // small enough that we don't consume too many open fds, especially on macOS.
        const MAX_UNSYNCED_WRITES: usize = 1;
        if self.unsynced.len() > MAX_UNSYNCED_WRITES {
            // TODO(cloutiertyler): I am intentionally not fsync-ing all the
            // files here because, that is prohibitively expensive. We need to
            // develop a better mechanism for batching writes to disk for blobs
            // before we can reasonably fsync.  Note this means that it's
            // possible for files to get lost if the operating system crashes
            // before they are synced. Not very database-y of us tsk tsk, but
            // fine for now.
            // self.sync_all().unwrap();
            self.unsynced.clear();
        }

        self.obj_size += bytes.len() as u64;
        self.map.insert(hash, bytes);

        hash
    }

    fn get(&self, hash: Hash) -> Option<bytes::Bytes> {
        self.map.get(&hash).map(|v| bytes::Bytes::from(v.clone()))
    }

    // NOTE: Flushing a `File` does nothing (just returns Ok(())), but flushing a BufWriter will
    // write the current buffer to the `File` by calling write. All `File` writes are atomic
    // so if you want to do an atomic action, make sure it all fits within the BufWriter buffer.
    // https://www.evanjones.ca/durability-filesystem.html
    // https://stackoverflow.com/questions/42442387/is-write-safe-to-be-called-from-multiple-threads-simultaneously/42442926#42442926
    // https://github.com/facebook/rocksdb/wiki/WAL-Performance
    fn flush(&mut self) -> Result<(), DBError> {
        // TODO if we start buffering
        Ok(())
    }

    // This will not return until the data is physically written to disk, as opposed to having
    // been pushed to the OS. You probably don't need to call this function, unless you need it
    // to be for sure durably written.
    // SEE: https://stackoverflow.com/questions/69819990/whats-the-difference-between-flush-and-sync-all
    fn sync_all(&mut self) -> Result<(), DBError> {
        for file in self.unsynced.drain(..) {
            file.sync_all()?;
        }
        Ok(())
    }

    fn size_on_disk(&self) -> Result<u64, DBError> {
        Ok(self.total_mem_size_bytes())
    }
}

fn hex_prefixes() -> Vec<String> {
    let mut prefixes = Vec::new();
    let hex = "0123456789abcdef";
    for h1 in hex.chars() {
        for h2 in hex.chars() {
            let mut path = String::new();
            path.push(h1);
            path.push(h2);
            prefixes.push(path);
        }
    }
    prefixes
}

#[cfg(test)]
mod tests {
    use crate::db::ostorage::{hashmap_object_db::HashMapObjectDB, ObjectDB};
    use crate::error::DBError;
    use crate::hash::hash_bytes;
    use spacetimedb_lib::error::ResultTest;
    use tempfile::TempDir;

    const TEST_DB_DIR_PREFIX: &str = "objdb_test";
    const TEST_DATA1: &[u8; 21] = b"this is a byte string";
    const TEST_DATA2: &[u8; 26] = b"this is also a byte string";

    fn setup() -> Result<(HashMapObjectDB, TempDir), DBError> {
        let tmp_dir = TempDir::with_prefix(TEST_DB_DIR_PREFIX).unwrap();
        let db = HashMapObjectDB::open(tmp_dir.path())?;
        Ok((db, tmp_dir))
    }

    #[test]
    fn test_add_and_get() -> ResultTest<()> {
        let (mut db, _tmp_dir) = setup()?;

        let hash1 = db.add(TEST_DATA1.to_vec());
        let hash2 = db.add(TEST_DATA2.to_vec());

        let result = db.get(hash1).unwrap();
        assert_eq!(TEST_DATA1.to_vec(), result);

        let result = db.get(hash2).unwrap();
        assert_eq!(TEST_DATA2.to_vec(), result);

        Ok(())
    }

    #[test]
    fn test_flush() -> ResultTest<()> {
        let (mut db, _tmp_dir) = setup()?;

        db.add(TEST_DATA1.to_vec());
        db.add(TEST_DATA2.to_vec());

        assert!(db.flush().is_ok());
        Ok(())
    }

    #[test]
    fn test_flush_sync_all() -> ResultTest<()> {
        let (mut db, _tmp_dir) = setup()?;

        db.add(TEST_DATA1.to_vec());
        db.add(TEST_DATA2.to_vec());

        assert!(db.sync_all().is_ok());
        Ok(())
    }

    #[test]
    fn test_miss() -> ResultTest<()> {
        let (mut db, _tmp_dir) = setup()?;

        let _hash2 = db.add(TEST_DATA2.to_vec());

        let hash = hash_bytes(TEST_DATA1);
        let result = db.get(hash);

        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_size() -> ResultTest<()> {
        let (mut db, _tmp_dir) = setup()?;

        let hash1 = db.add(TEST_DATA1.to_vec());
        db.add(TEST_DATA1.to_vec());

        assert_eq!(db.total_key_size_bytes(), hash1.data.len() as u64);
        assert_eq!(db.total_obj_size_bytes(), TEST_DATA1.len() as u64);
        assert_eq!(db.total_mem_size_bytes(), (TEST_DATA1.len() + hash1.data.len()) as u64);

        let hash2 = db.add(TEST_DATA2.to_vec());
        assert_eq!(db.total_key_size_bytes(), (hash1.data.len() + hash2.data.len()) as u64);
        assert_eq!(db.total_obj_size_bytes(), (TEST_DATA1.len() + TEST_DATA2.len()) as u64);
        assert_eq!(
            db.total_mem_size_bytes(),
            (TEST_DATA1.len() + TEST_DATA2.len() + hash1.data.len() + hash2.data.len()) as u64
        );
        Ok(())
    }
}
