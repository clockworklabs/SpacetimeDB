use crate::hash::{hash_bytes, Hash};

pub struct ObjectDb {
    db: sled::Db,
}

impl ObjectDb {
    pub fn init() -> Result<Self, anyhow::Error> {
        let config = sled::Config::default()
            .path("/stdb/control_node/object_db")
            .flush_every_ms(Some(50))
            .mode(sled::Mode::HighThroughput);
        let db = config.open()?;
        Ok(Self { db })
    }

    pub fn get_object(&self, hash: &Hash) -> Result<Option<Vec<u8>>, anyhow::Error> {
        let value = self.db.get(hash.as_slice())?;
        if let Some(value) = value {
            Ok(Some(value.to_vec()))
        } else {
            Ok(None)
        }
    }

    pub fn insert_object(&self, bytes: Vec<u8>) -> Result<(), anyhow::Error> {
        let hash = hash_bytes(&bytes);
        self.db.insert(hash.as_slice(), bytes)?;
        Ok(())
    }
}
