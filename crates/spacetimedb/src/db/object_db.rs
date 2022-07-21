use super::super::hash::{hash_bytes, Hash};
use hex;
use std::os::unix::prelude::MetadataExt;
use std::{
    collections::HashMap,
    fs::{self, read_dir, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub struct ObjectDB {
    root: PathBuf,
    map: HashMap<Hash, Vec<u8>>,
    obj_size: u64,
    unsynced: Vec<File>,
}

impl ObjectDB {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, anyhow::Error> {
        let root = path.as_ref();
        fs::create_dir_all(root).unwrap();

        let mut cache: HashMap<Hash, Vec<u8>> = HashMap::new();
        let mut obj_size: u64 = 0;

        for item in read_dir(root)? {
            let dir_entry = item?;
            let path = dir_entry.path();
            let dir_name = path.file_name().unwrap().to_str().unwrap();
            let hex_dir_name = hex::decode(dir_name);
            if let Err(err) = hex_dir_name {
                log::warn!("invalid object dir found: {:?}", err);
                continue;
            }
            let hex_dir_bytes = hex_dir_name.unwrap();
            if hex_dir_bytes.len() != 1 {
                log::warn!("invalid object dir found, name longer than 1");
                continue;
            }
            let first_byte: u8 = hex_dir_bytes[0];

            let inner_dir = &PathBuf::from(root).join(path);
            for item in read_dir(inner_dir)? {
                let dir_entry = item?;
                let size = dir_entry.metadata()?.size();
                let path = dir_entry.path();
                let dir_name = path.file_name().unwrap().to_str().unwrap();
                let hex_dir_name = hex::decode(dir_name);
                if let Err(err) = hex_dir_name {
                    log::warn!("invalid object dir found: {:?}", err);
                    continue;
                }
                let hex_dir_bytes = hex_dir_name.unwrap();
                if hex_dir_bytes.len() != 31 {
                    log::warn!("invalid object dir found, name longer than 31");
                    continue;
                }
                let mut bytes: [u8; 32] = [0; 32];
                bytes[0] = first_byte;
                bytes[1..].copy_from_slice(&hex_dir_bytes);

                let mut file = OpenOptions::new().read(true).open(inner_dir.join(path))?;

                let mut contents = Vec::new();
                file.read_to_end(&mut contents).unwrap();

                let hash = Hash::from_slice(&bytes);
                cache.insert(*hash, contents);
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

    pub fn add(&mut self, bytes: Vec<u8>) -> Hash {
        let hash = hash_bytes(&bytes);
        if self.map.contains_key(&hash) {
            return hash;
        }

        let folder = hex::encode(&hash[0..1]);
        let filename = hex::encode(&hash[1..]);
        let path = self.root.join(folder).join(filename);

        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap()
        }
        let mut unsynced = OpenOptions::new().write(true).create(true).open(path).unwrap();
        unsynced.write_all(&bytes).unwrap();
        self.unsynced.push(unsynced);

        if self.unsynced.len() > 128 {
            self.sync_all().unwrap();
        }

        self.obj_size += bytes.len() as u64;
        self.map.insert(hash, bytes);

        hash
    }

    pub fn get(&self, hash: Hash) -> Option<&[u8]> {
        self.map.get(&hash).map(|v| &v[..])
    }

    // NOTE: Flushing a `File` does nothing (just returns Ok(())), but flushing a BufWriter will
    // write the current buffer to the `File` by calling write. All `File` writes are atomic
    // so if you want to do an atomic action, make sure it all fits within the BufWriter buffer.
    // https://www.evanjones.ca/durability-filesystem.html
    // https://stackoverflow.com/questions/42442387/is-write-safe-to-be-called-from-multiple-threads-simultaneously/42442926#42442926
    // https://github.com/facebook/rocksdb/wiki/WAL-Performance
    pub fn flush(&mut self) -> Result<(), anyhow::Error> {
        // TODO if we start buffering
        Ok(())
    }

    // This will not return until the data is physically written to disk, as opposed to having
    // been pushed to the OS. You probably don't need to call this function, unless you need it
    // to be for sure durably written.
    // SEE: https://stackoverflow.com/questions/69819990/whats-the-difference-between-flush-and-sync-all
    pub fn sync_all(&mut self) -> Result<(), anyhow::Error> {
        for file in self.unsynced.drain(..) {
            file.sync_all()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::db::object_db::ObjectDB;
    use crate::hash::hash_bytes;
    use anyhow::Error;
    use tempdir::TempDir;

    const TEST_DB_DIR_PREFIX: &str = "objdb_test";
    const TEST_DATA1: &[u8; 21] = b"this is a byte string";
    const TEST_DATA2: &[u8; 26] = b"this is also a byte string";

    fn setup() -> Result<ObjectDB, Error> {
        let tmp_dir = TempDir::new(TEST_DB_DIR_PREFIX).unwrap();
        ObjectDB::open(tmp_dir.path())
    }

    #[test]
    fn test_add_and_get() {
        let mut db = setup().unwrap();

        let hash1 = db.add(TEST_DATA1.to_vec());
        let hash2 = db.add(TEST_DATA2.to_vec());

        let result = db.get(hash1).unwrap();
        assert_eq!(TEST_DATA1, result);

        let result = db.get(hash2).unwrap();
        assert_eq!(TEST_DATA2, result);
    }

    #[test]
    fn test_flush() {
        let mut db = setup().unwrap();

        db.add(TEST_DATA1.to_vec());
        db.add(TEST_DATA2.to_vec());

        assert!(db.flush().is_ok());
    }

    #[test]
    fn test_flush_sync_all() {
        let mut db = setup().unwrap();

        db.add(TEST_DATA1.to_vec());
        db.add(TEST_DATA2.to_vec());

        assert!(db.sync_all().is_ok());
    }

    #[test]
    fn test_miss() {
        let mut db = setup().unwrap();

        let _hash2 = db.add(TEST_DATA2.to_vec());

        let hash = hash_bytes(TEST_DATA1);
        let result = db.get(hash);

        assert!(result.is_none());
    }

    #[test]
    fn test_size() {
        let mut db = setup().unwrap();

        let hash1 = db.add(TEST_DATA1.to_vec());
        db.add(TEST_DATA1.to_vec());

        assert_eq!(db.total_key_size_bytes(), hash1.len() as u64);
        assert_eq!(db.total_obj_size_bytes(), TEST_DATA1.len() as u64);
        assert_eq!(db.total_mem_size_bytes(), (TEST_DATA1.len() + hash1.len()) as u64);

        let hash2 = db.add(TEST_DATA2.to_vec());
        assert_eq!(db.total_key_size_bytes(), (hash1.len() + hash2.len()) as u64);
        assert_eq!(db.total_obj_size_bytes(), (TEST_DATA1.len() + TEST_DATA2.len()) as u64);
        assert_eq!(
            db.total_mem_size_bytes(),
            (TEST_DATA1.len() + TEST_DATA2.len() + hash1.len() + hash2.len()) as u64
        );
    }
}
