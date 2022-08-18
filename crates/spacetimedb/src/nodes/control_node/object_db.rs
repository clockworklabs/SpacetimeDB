use crate::hash::{hash_bytes, Hash};

// TODO: Consider making not static
lazy_static::lazy_static! {
    static ref CONTROL_NODE_OBJECT_DB: sled::Db = init().unwrap();
}

fn init() -> Result<sled::Db, anyhow::Error> {
    let config = sled::Config::default()
        .path("/stdb/control_node/object_db")
        .flush_every_ms(Some(50))
        .mode(sled::Mode::HighThroughput);
    let db = config.open()?;
    Ok(db)
}

pub async fn get_object(hash: &Hash) -> Result<Option<Vec<u8>>, anyhow::Error> {
    let value = CONTROL_NODE_OBJECT_DB.get(hash.as_slice())?;
    if let Some(value) = value {
        Ok(Some(value.to_vec()))
    } else {
        Ok(None)
    }
}

pub async fn insert_object(bytes: Vec<u8>) -> Result<(), anyhow::Error> {
    let hash = hash_bytes(&bytes);
    CONTROL_NODE_OBJECT_DB.insert(hash.as_slice(), bytes)?;
    Ok(())
}
