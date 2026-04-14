use core::hash::{BuildHasher, BuildHasherDefault};
use nohash_hasher::BuildNoHashHasher;

pub use hashbrown::Equivalent;
pub use nohash_hasher::IsEnabled as ValidAsIdentityHash;

pub mod hash_set {
    pub use super::HashSet;
    pub use hashbrown::hash_set::*;
}

pub mod hash_map {
    pub use super::HashMap;
    pub use hashbrown::hash_map::*;
}

pub type DefaultHashBuilder = BuildHasherDefault<ahash::AHasher>;
// TODO(centril): expose two maps instead,
// one `map::fast::HashMap` and one `map::ddos::HashMap`.
// In the first case we won't care about DDoS protection at all and can use `foldhash::fast`.
// In the lattr, we can use e.g., randomized AHash.
pub type HashMap<K, V> = hashbrown::HashMap<K, V, DefaultHashBuilder>;
pub type HashSet<T> = hashbrown::HashSet<T, DefaultHashBuilder>;

/// A version of [`HashMap<K, V>`] using the identity hash function,
/// which is valid for any key type that can be converted to a `u64` without truncation.
pub type IntMap<K, V> = hashbrown::HashMap<K, V, BuildNoHashHasher<K>>;

/// A version of [`HashSet<K>`] using the identity hash function,
/// which is valid for any key type that can be converted to a `u64` without truncation.
pub type IntSet<K> = hashbrown::HashSet<K, BuildNoHashHasher<K>>;

pub trait HashCollectionExt {
    /// Returns a new collection with default capacity, using `S::default()` to build the hasher.
    fn new() -> Self;

    /// Returns a new collection with `capacity`, using `S::default()` to build the hasher.
    fn with_capacity(capacity: usize) -> Self;
}

impl<K, V, S: BuildHasher + Default> HashCollectionExt for hashbrown::HashMap<K, V, S> {
    fn new() -> Self {
        Self::with_hasher(S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, S::default())
    }
}

impl<K, S: BuildHasher + Default> HashCollectionExt for hashbrown::HashSet<K, S> {
    fn new() -> Self {
        Self::with_hasher(S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, S::default())
    }
}
