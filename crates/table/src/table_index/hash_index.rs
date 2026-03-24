use super::{
    key_size::KeyBytesStorage,
    same_key_entry::{same_key_iter, SameKeyEntry, SameKeyEntryIter},
    Index, KeySize,
};
use crate::indexes::RowPointer;
use core::borrow::Borrow;
use core::hash::Hash;
use spacetimedb_data_structures::map::hash_map::EntryRef;
use spacetimedb_sats::memory_usage::MemoryUsage;

// Faster than ahash, so we use this explicitly.
use foldhash::fast::RandomState;
use hashbrown::HashMap;

/// A multi map that relates a `K` to a *set* of `RowPointer`s.
#[derive(Debug, PartialEq, Eq)]
pub struct HashIndex<K: Eq + Hash> {
    /// The map is backed by a `HashMap` for relating keys to values.
    ///
    /// A value set is stored as a `SmallVec`.
    /// This is an optimization over a `Vec<_>`
    /// as we allow a single element to be stored inline
    /// to improve performance for the common case of one element.
    map: HashMap<K, SameKeyEntry, RandomState>,
    /// The memoized number of rows indexed in `self.map`.
    num_rows: usize,
    /// Storage for [`Index::num_key_bytes`].
    num_key_bytes: u64,
}

impl<K: Eq + Hash> Default for HashIndex<K> {
    fn default() -> Self {
        Self {
            map: <_>::default(),
            num_rows: <_>::default(),
            num_key_bytes: <_>::default(),
        }
    }
}

impl<K: MemoryUsage + Eq + Hash> MemoryUsage for HashIndex<K> {
    fn heap_usage(&self) -> usize {
        let Self {
            map,
            num_rows,
            num_key_bytes,
        } = self;
        map.heap_usage() + num_rows.heap_usage() + num_key_bytes.heap_usage()
    }
}

impl<K: KeySize + Eq + Hash> Index for HashIndex<K> {
    type Key = K;

    fn clone_structure(&self) -> Self {
        <_>::default()
    }

    /// Inserts the relation `key -> ptr` to this multimap.
    ///
    /// The map does not check whether `key -> ptr` was already in the map.
    /// It's assumed that the same `ptr` is never added twice,
    /// and multimaps do not bind one `key` to the same `ptr`.
    fn insert(&mut self, key: Self::Key, ptr: RowPointer) -> Result<(), RowPointer> {
        self.num_rows += 1;
        self.num_key_bytes.add_to_key_bytes(&key);
        self.map.entry(key).or_default().push(ptr);
        Ok(())
    }

    /// Deletes `key -> ptr` from this multimap.
    ///
    /// Returns whether `key -> ptr` was present.
    fn delete(&mut self, key: &K, ptr: RowPointer) -> bool {
        self.delete(key, ptr)
    }

    type PointIter<'a>
        = SameKeyEntryIter<'a>
    where
        Self: 'a;

    fn seek_point(&self, point: &Self::Key) -> Self::PointIter<'_> {
        self.seek_point(point)
    }

    fn num_keys(&self) -> usize {
        self.map.len()
    }

    fn num_rows(&self) -> usize {
        self.num_rows
    }

    /// Deletes all entries from the multimap, leaving it empty.
    /// This will not deallocate the outer map.
    fn clear(&mut self) {
        self.map.clear();
        self.num_rows = 0;
        self.num_key_bytes.reset_to_zero();
    }

    fn can_merge(&self, _: &Self, _: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        // `self.insert` always returns `Ok(_)`.
        Ok(())
    }
}

impl<K: KeySize + Eq + Hash> HashIndex<K> {
    /// Returns an iterator over keys that have more than one row pointer,
    /// yielding `(&key, count)` for each duplicate key.
    pub(super) fn iter_duplicates(&self) -> impl Iterator<Item = (&K, usize)> {
        self.map.iter().filter_map(|(k, entry)| {
            let count = entry.count();
            if count > 1 {
                Some((k, count))
            } else {
                None
            }
        })
    }

    /// Check for duplicates and, if none, convert into a `HashMap<K, RowPointer>`.
    ///
    /// Returns `Ok(map)` if every key maps to exactly one row.
    /// Returns `Err((self, ptr))` with a witness `RowPointer` of a duplicate if any key
    /// maps to more than one row. The original `HashIndex` is returned intact on error.
    pub(super) fn check_and_into_unique(self) -> Result<HashMap<K, RowPointer, RandomState>, (Self, RowPointer)> {
        // First pass: check for duplicates.
        let dup = self
            .map
            .values()
            .find_map(|entry| {
                if entry.count() > 1 {
                    Some(entry.iter().next().unwrap())
                } else {
                    None
                }
            });

        if let Some(ptr) = dup {
            return Err((self, ptr));
        }

        // No duplicates; conversion is infallible.
        let result = self
            .map
            .into_iter()
            .map(|(k, entry)| {
                let ptr = entry.iter().next().unwrap();
                (k, ptr)
            })
            .collect();

        Ok(result)
    }

    /// See [`Index::delete`].
    ///
    /// This version has relaxed bounds
    /// where relaxed means that the key type can be borrowed from the index's key type
    /// and need not be `Index::Key` itself.
    /// This allows e.g., queries with `&str` rather than providing an owned string key.
    /// This can be exploited to avoid heap alloctions in some situations,
    /// e.g., borrowing the input directly from BSATN.
    /// This is similar to the bounds on [`HashMap::remove`].
    pub fn delete<Q>(&mut self, key: &Q, ptr: RowPointer) -> bool
    where
        Q: ?Sized + KeySize + Hash + Eq,
        <Self as Index>::Key: Borrow<Q>,
    {
        let EntryRef::Occupied(mut entry) = self.map.entry_ref(key) else {
            return false;
        };

        let (deleted, is_empty) = entry.get_mut().delete(ptr);

        if deleted {
            self.num_rows -= 1;
            self.num_key_bytes.sub_from_key_bytes(entry.key());
        }

        if is_empty {
            entry.remove();
        }

        deleted
    }

    /// See [`Index::seek_point`].
    ///
    /// This version has relaxed bounds
    /// where relaxed means that the key type can be borrowed from the index's key type
    /// and need not be `Index::Key` itself.
    /// This allows e.g., queries with `&str` rather than providing an owned string key.
    /// This can be exploited to avoid heap alloctions in some situations,
    /// e.g., borrowing the input directly from BSATN.
    /// This is similar to the bounds on [`HashMap::get`].
    pub fn seek_point<Q: ?Sized + Eq + Hash>(&self, point: &Q) -> <Self as Index>::PointIter<'_>
    where
        <Self as Index>::Key: Borrow<Q>,
    {
        same_key_iter(self.map.get(point))
    }
}
