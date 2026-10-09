use std::{mem, sync::Arc};

use spacetimedb_data_structures::small_map::SmallHashMap;
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_sats::layout::RowTypeLayout;

use crate::{
    blob_store::BlobHash,
    indexes::PageOffset,
    page::Page,
    row_type_visitor::VarLenVisitorProgram,
    tiered::{
        blob_manager::{BlobHandle, BlobManager},
        error::BlobError,
        map::{ObjectKey, ObjectMap},
    },
    var_len::VarLenMembers,
};

#[derive(Debug)]
pub struct BlobSet {
    manager: Arc<BlobManager>,
    entries: ObjectMap<BlobUseEntry>,
}

#[derive(Debug, PartialEq, Eq)]
struct BlobUseEntry {
    uses: usize,
    size: u64,
}

impl BlobUseEntry {
    fn inc_uses(&mut self) {
        self.uses += 1;
    }

    fn dec_uses(&mut self) {
        self.uses = self.uses.saturating_sub(1);
    }
}

impl BlobSet {
    pub fn new_for_test() -> Self {
        Self {
            manager: BlobManager::new_for_test().into(),
            entries: <_>::default(),
        }
    }

    pub fn insert(&mut self, bytes: &[u8]) -> Result<BlobHash, BlobError> {
        let hash = BlobHash::hash_from_bytes(bytes);
        self.entries
            .entry(hash.into())
            .and_modify(BlobUseEntry::inc_uses)
            .or_try_insert_with(|| {
                self.manager.get_or_install(hash, bytes, 0).map(|_| BlobUseEntry {
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

        self.manager.get_or_fault(hash)
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

    pub fn physical_bytes_used_by_blobs(&self) -> u64 {
        self.entries
            .iter()
            .map(|(_, entry)| entry.size + mem::size_of::<ObjectKey>() as u64)
            .sum()
    }

    pub fn bytes_used_by_blobs(&self) -> u64 {
        self.entries
            .iter()
            .map(|(_, entry)| entry.size * entry.uses as u64)
            .sum()
    }

    pub fn num_blobs(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
impl BlobSet {
    pub fn usage_counter(&self) -> spacetimedb_data_structures::map::HashMap<BlobHash, usize> {
        self.entries
            .iter()
            .map(|(hash, entry)| (BlobHash::from(*hash), entry.uses))
            .collect()
    }

    pub fn entries(&self) -> &ObjectMap<BlobUseEntry> {
        &self.entries
    }
}

impl MemoryUsage for BlobSet {
    fn heap_usage(&self) -> usize {
        todo!()
    }
}

/// A set of resident blobs for a specific row.
#[derive(Default)]
pub struct BlobReadSet {
    inner: SmallHashMap<BlobHash, BlobHandle, 4, 16>,
}

impl BlobReadSet {
    pub fn new(
        visitor: &VarLenVisitorProgram,
        page: &Page,
        blobs: &BlobSet,
        layout: &RowTypeLayout,
        offset: PageOffset,
    ) -> Result<Self, BlobError> {
        let row_data = page.get_row_data(offset, layout.size());
        let mut inner = SmallHashMap::default();
        for vlr in unsafe { visitor.visit_var_len(row_data) }.filter(|vlr| vlr.is_large_blob()) {
            let granule = unsafe { page.iter_var_len_object(vlr.first_granule) }.next().unwrap();
            let blob_hash = granule.blob_hash();
            let blob = blobs.read(blob_hash)?;
            inner.insert(blob_hash, blob);
        }

        Ok(Self { inner })
    }

    pub fn get(&self, hash: &BlobHash) -> Option<&BlobHandle> {
        self.inner.get(hash)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&BlobHash, &BlobHandle)> {
        self.inner.iter()
    }
}
