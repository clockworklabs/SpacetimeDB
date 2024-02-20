//! Provides the interface [`BlobStore`] that tables use to talk to
//! a blob store engine for large var-len objects.
//!
//! These blob objects are referred to by their [`BlobHash`],
//! which is currently defined through BLAKE3 on the bytes of the blob object.
//!
//! Two simple implementations are provided,
//! primarily for tests and benchmarking.
//! - [`NullBlobStore`], a blob store that always panics.
//!   Used when ensuring that the blob store is unreachable in a scenario.
//! - [`HashMapBlobStore`], a blob store backed by a `HashMap` that refcounts blob objects.
//!   It is not optimize and is mainly intended for testing purposes.

use blake3::hash;
use std::collections::{hash_map::Entry, HashMap};

/// The content address of a blob-stored object.
#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
pub struct BlobHash {
    /// The hash of the blob-stored object.
    ///
    /// Uses BLAKE3 which fits in 32 bytes.
    pub data: [u8; Self::SIZE],
}

impl BlobHash {
    /// The size of the hash function's output in bytes.
    pub const SIZE: usize = 32;

    /// Returns the blob hash for `bytes`.
    fn hash_from_bytes(bytes: &[u8]) -> Self {
        let data = hash(bytes).into();
        Self { data }
    }
}

impl TryFrom<&[u8]> for BlobHash {
    type Error = ();

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let data: [u8; Self::SIZE] = data.try_into().map_err(drop)?;
        Ok(Self { data })
    }
}

/// An error that signifies that a [`BlobHash`] wasn't associated with a large blob object.
#[derive(Debug)]
pub struct NoSuchBlobError;

/// The interface that tables use to talk to the blob store engine for large var-len objects.
///
/// These blob objects are referred to by their [`BlobHash`],
/// which is currently defined through BLAKE3 on the bytes of the blob object.
pub trait BlobStore {
    /// Mark the `hash` as used.
    ///
    /// This is a more efficient way of doing:
    /// ```ignore
    /// let bytes = self.retrieve_blob(&hash);
    /// let _ = self.insert_blob(&bytes);
    /// ```
    fn clone_blob(&mut self, hash: &BlobHash) -> Result<(), NoSuchBlobError>;

    /// Insert `bytes` into the blob store.
    ///
    /// Returns the content address of `bytes` a `BlobHash`
    /// which can be used in [`retrieve_blob`] to fetch it.
    fn insert_blob(&mut self, bytes: &[u8]) -> BlobHash;

    /// Returns the bytes stored at the content address `hash`.
    fn retrieve_blob(&self, hash: &BlobHash) -> Result<&[u8], NoSuchBlobError>;

    /// Marks the `hash` as unused.
    ///
    /// Depending on the strategy employed by the blob store,
    /// this might not actually free the data,
    /// but rather just decrement a reference count.
    fn free_blob(&mut self, hash: &BlobHash) -> Result<(), NoSuchBlobError>;
}

/// A blob store that panics on all operations.
/// Used for tests when you want to ensure that the blob store isn't used.
#[derive(Default)]
pub struct NullBlobStore;

impl BlobStore for NullBlobStore {
    fn clone_blob(&mut self, _hash: &BlobHash) -> Result<(), NoSuchBlobError> {
        unimplemented!("NullBlobStore doesn't do anything")
    }

    fn insert_blob(&mut self, _bytes: &[u8]) -> BlobHash {
        unimplemented!("NullBlobStore doesn't do anything")
    }
    fn retrieve_blob(&self, _hash: &BlobHash) -> Result<&[u8], NoSuchBlobError> {
        unimplemented!("NullBlobStore doesn't do anything")
    }

    fn free_blob(&mut self, _hash: &BlobHash) -> Result<(), NoSuchBlobError> {
        unimplemented!("NullBlobStore doesn't do anything")
    }
}

/// A blob store that is backed by a hash map with a reference counted value.
/// Used for tests when you need an actual blob store.
#[derive(Default)]
pub struct HashMapBlobStore {
    /// For testing, we use a hash map with a reference count
    /// to handle freeing and cloning correctly.
    map: HashMap<BlobHash, BlobObject>,
}

/// A blob object including a reference count and the data.
struct BlobObject {
    /// Reference count of the blob.
    uses: usize,
    /// The blob data.
    blob: Box<[u8]>,
}

impl BlobStore for HashMapBlobStore {
    fn clone_blob(&mut self, hash: &BlobHash) -> Result<(), NoSuchBlobError> {
        self.map.get_mut(hash).ok_or(NoSuchBlobError)?.uses += 1;
        Ok(())
    }

    fn insert_blob(&mut self, bytes: &[u8]) -> BlobHash {
        let hash = BlobHash::hash_from_bytes(bytes);
        self.map
            .entry(hash)
            .and_modify(|v| v.uses += 1)
            .or_insert_with(|| BlobObject {
                blob: bytes.into(),
                uses: 1,
            });
        hash
    }

    fn retrieve_blob(&self, hash: &BlobHash) -> Result<&[u8], NoSuchBlobError> {
        self.map.get(hash).map(|obj| &*obj.blob).ok_or(NoSuchBlobError)
    }

    fn free_blob(&mut self, hash: &BlobHash) -> Result<(), NoSuchBlobError> {
        match self.map.entry(*hash) {
            Entry::Vacant(_) => return Err(NoSuchBlobError),
            Entry::Occupied(entry) if entry.get().uses == 1 => drop(entry.remove()),
            Entry::Occupied(mut entry) => entry.get_mut().uses -= 1,
        }
        Ok(())
    }
}

#[cfg(test)]
impl HashMapBlobStore {
    /// Returns an iterator over the (hash, usage count, blob bytes) triple.
    fn iter(&self) -> impl Iterator<Item = (&BlobHash, usize, &[u8])> + '_ {
        self.map.iter().map(|(hash, obj)| (hash, obj.uses, &*obj.blob))
    }

    /// Returns a map relating blob hashes to the usage count in this blob store.
    pub fn usage_counter(&self) -> HashMap<BlobHash, usize> {
        self.iter().map(|(hash, uses, _)| (*hash, uses)).collect()
    }
}
