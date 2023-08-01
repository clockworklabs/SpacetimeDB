use std::collections::HashMap;

use bytes::Bytes;
use parking_lot::RwLock;
use spacetimedb_lib::{data_key, hash::hash_bytes, DataKey, Hash};

use super::traits;

pub struct Blob {
    blob: Bytes,
}

impl traits::Blob for Blob {
    fn view(&self) -> &[u8] {
        &self.blob
    }
}

#[derive(Clone)]
pub struct BlobRef {
    blob: Bytes,
}

impl BlobRef {
    fn new(blob: Bytes) -> Self {
        Self { blob }
    }

    pub fn view(&self) -> &[u8] {
        &self.blob
    }
}

pub struct Memory {
    odb: RwLock<ObjectStore>,
}

impl Memory {
    pub fn new() -> Self {
        Self {
            odb: RwLock::new(ObjectStore::new()),
        }
    }

    pub fn get(&self, data_key: &DataKey) -> BlobRef {
        let blob = { self.odb.read().get(data_key).blob };
        BlobRef::new(blob)
    }

    pub fn insert(&self, bytes: &[u8]) -> DataKey {
        self.odb.write().insert(bytes)
    }

    pub fn blob_to_owned(&self, blob_ref: BlobRef) -> Blob {
        Blob { blob: blob_ref.blob }
    }
}

struct ObjectStore {
    objects: HashMap<Hash, Bytes>,
}

impl ObjectStore {
    fn new() -> Self {
        let objects = HashMap::new();
        Self { objects }
    }

    fn get(&self, data_key: &DataKey) -> BlobRef {
        let data = match data_key {
            DataKey::Data(data) => Bytes::copy_from_slice(data),
            DataKey::Hash(hash) => self.objects.get(hash).unwrap().clone(),
        };
        BlobRef::new(data)
    }

    fn insert(&mut self, bytes: &[u8]) -> DataKey {
        match data_key::InlineData::from_bytes(bytes) {
            Some(inline) => DataKey::Data(inline),
            None => {
                let hash = hash_bytes(bytes);
                self.objects.insert(hash, Bytes::copy_from_slice(bytes));
                DataKey::Hash(spacetimedb_lib::Hash::from_arr(&hash.data))
            }
        }
    }
}
