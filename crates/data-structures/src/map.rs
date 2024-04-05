use core::hash::BuildHasher;
pub use hashbrown::hash_map::{DefaultHashBuilder, Entry, RawEntryMut};
pub use hashbrown::{HashMap, HashSet};
use nohash_hasher::BuildNoHashHasher;
pub use nohash_hasher::IsEnabled as ValidAsIdentityHash;

/// A version of [`HashMap<K, V>`] using the identity hash function,
/// which is valid for any key type that can be converted to a `u64` without truncation.
pub type IntMap<K, V> = HashMap<K, V, BuildNoHashHasher<K>>;

/// A version of [`HashSet<K>`] using the identity hash function,
/// which is valid for any key type that can be converted to a `u64` without truncation.
pub type IntSet<K> = HashSet<K, BuildNoHashHasher<K>>;

pub trait HashCollectionExt {
    /// Returns a new collection with default capacity, using `S::default()` to build the hasher.
    fn new() -> Self;

    /// Returns a new collection with `capacity`, using `S::default()` to build the hasher.
    fn with_capacity(capacity: usize) -> Self;
}

impl<K, V, S: BuildHasher + Default> HashCollectionExt for HashMap<K, V, S> {
    fn new() -> Self {
        HashMap::with_hasher(S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        HashMap::with_capacity_and_hasher(capacity, S::default())
    }
}

impl<K, S: BuildHasher + Default> HashCollectionExt for HashSet<K, S> {
    fn new() -> Self {
        HashSet::with_hasher(S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        HashSet::with_capacity_and_hasher(capacity, S::default())
    }
}
