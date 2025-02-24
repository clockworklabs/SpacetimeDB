use core::hash::{BuildHasher, BuildHasherDefault};
pub use hashbrown::hash_map::Entry;
use nohash_hasher::BuildNoHashHasher;
pub use nohash_hasher::IsEnabled as ValidAsIdentityHash;

pub type DefaultHashBuilder = BuildHasherDefault<ahash::AHasher>;
// TODO(centril): expose two maps instead,
// one `map::fast::HashMap` and one `map::ddos::HashMap`.
// In the first case we won't care about DDoS protection at all and can use `foldhash::fast`.
// In the lattr, we can use e.g., randomized AHash.
pub type HashMap<K, V, S = DefaultHashBuilder> = hashbrown::HashMap<K, V, S>;
pub type HashSet<T, S = DefaultHashBuilder> = hashbrown::HashSet<T, S>;

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
