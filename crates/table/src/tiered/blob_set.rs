use std::sync::Arc;

use crate::{
    blob_store::BlobHash,
    tiered::{
        blob_manager::{BlobHandle, BlobManager},
        error::BlobError,
        map::ObjectMap,
    },
};

pub struct BlobSet {
    manager: Arc<BlobManager>,
    entries: ObjectMap<BlobUseEntry>,
}

struct BlobUseEntry {
    uses: usize,
    size: u64,
}

impl BlobUseEntry {
    fn inc_uses(&mut self) {
        self.uses += 1;
    }

    fn dec_uses(&mut self) {
        self.uses -= 1;
    }
}

impl BlobSet {
    pub fn insert(&mut self, bytes: &[u8]) -> Result<BlobHash, BlobError> {
        let hash = BlobHash::hash_from_bytes(bytes);
        self.entries
            .entry(hash.into())
            .and_modify(BlobUseEntry::inc_uses)
            .or_try_insert_with(|| {
                self.manager
                    .install_or_get_resident(hash, bytes, 0)
                    .map(|_| BlobUseEntry {
                        uses: 1,
                        size: bytes.len() as u64,
                    })
            })
            .map(|_| hash)
    }

    pub fn clone_ref(&mut self, hash: BlobHash) -> Result<(), BlobError> {
        self.entries
            .entry(hash.into())
            .and_modify(BlobUseEntry::inc_uses)
            .or_error(|| BlobError::MissingBlob(hash))
            .map(drop)
    }

    pub fn free_ref(&mut self, hash: BlobHash) -> Result<(), BlobError> {
        self.entries
            .entry(hash.into())
            .and_modify(BlobUseEntry::dec_uses)
            .or_error(|| BlobError::MissingBlob(hash))
            .map(drop)
    }

    pub fn read(&self, hash: BlobHash) -> Result<BlobHandle, BlobError> {
        if !self.entries.contains_key(&hash.into()) {
            return Err(BlobError::MissingBlob(hash));
        }

        self.manager.get(hash)
    }

    /*
    // Crate-private counterpart used by RowRef construction.
    fn read_at(&self, hash: BlobHash, access_epoch: u64) -> Result<BlobReadGuard, BlobError> {
        todo!()
    }

    pub fn iter_snapshot_metadata(&self) -> impl Iterator<Item = BlobSnapshotEntry> + '_ {
        todo!()
    }

    pub fn prepare_snapshot(&self) -> Result<PreparedBlobSnapshot, BlobError> {
        todo!()
    }
    */
}
