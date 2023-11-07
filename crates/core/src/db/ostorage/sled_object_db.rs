use crate::db::ostorage::ObjectDB;
use crate::error::DBError;
use crate::hash::{hash_bytes, Hash};
use bytes::Bytes;
use sled;
use sled::Mode::HighThroughput;
use std::path::Path;

pub struct SledObjectDB {
    db: sled::Db,
}

impl SledObjectDB {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DBError> {
        let config = sled::Config::default()
            .path(path)
            .flush_every_ms(Some(50))
            .mode(HighThroughput);
        let db = config.open()?;
        Ok(Self { db })
    }
}

impl ObjectDB for SledObjectDB {
    fn add(&self, bytes: &[u8]) -> Hash {
        let hash = hash_bytes(bytes);

        self.db.insert(hash.data.as_slice(), bytes).unwrap();

        hash
    }

    fn get(&self, hash: Hash) -> Option<Bytes> {
        match self.db.get(hash.as_slice()) {
            Ok(v) => v.map(|v| bytes::Bytes::from(v.to_vec())),
            Err(_) => None,
        }
    }

    fn flush(&self) -> Result<(), DBError> {
        match self.db.flush() {
            Ok(_) => Ok(()),
            Err(e) => Err(DBError::SledDbError(e)),
        }
    }

    fn sync_all(&self) -> Result<(), DBError> {
        self.flush()
    }

    fn size_on_disk(&self) -> Result<u64, DBError> {
        Ok(self.db.size_on_disk()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::db::ostorage::sled_object_db::SledObjectDB;
    use crate::db::ostorage::ObjectDB;

    use crate::error::DBError;
    use crate::hash::hash_bytes;
    use tempfile::TempDir;

    const TEST_DB_DIR_PREFIX: &str = "sledb_test";
    const TEST_DATA1: &[u8; 21] = b"this is a byte string";
    const TEST_DATA2: &[u8; 26] = b"this is also a byte string";

    fn setup() -> Result<SledObjectDB, DBError> {
        let tmp_dir = TempDir::with_prefix(TEST_DB_DIR_PREFIX).unwrap();
        SledObjectDB::open(tmp_dir.path())
    }

    #[test]
    fn test_add_and_get() {
        let db = setup().unwrap();

        let hash1 = db.add(TEST_DATA1);
        let hash2 = db.add(TEST_DATA2);

        let result = db.get(hash1).unwrap();
        assert_eq!(TEST_DATA1, result.to_vec().as_slice());

        let result = db.get(hash2).unwrap();
        assert_eq!(TEST_DATA2, result.to_vec().as_slice());
    }

    #[test]
    fn test_flush() {
        let db = setup().unwrap();

        db.add(TEST_DATA1);
        db.add(TEST_DATA2);

        assert!(db.flush().is_ok());
    }

    #[test]
    fn test_flush_sync_all() {
        let db = setup().unwrap();

        db.add(TEST_DATA1);
        db.add(TEST_DATA2);

        assert!(db.sync_all().is_ok());
    }

    #[test]
    fn test_miss() {
        let db = setup().unwrap();

        let _hash2 = db.add(TEST_DATA2);

        let hash = hash_bytes(TEST_DATA1);
        let result = db.get(hash);

        assert!(result.is_none());
    }
}
