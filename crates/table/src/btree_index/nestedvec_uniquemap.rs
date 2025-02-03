use crate::indexes::PageIndex;
use crate::indexes::PageOffset;
use crate::indexes::RowPointer;
use crate::indexes::SquashedOffset;
use crate::MemoryUsage;
use core::ops::Bound;
use core::ops::RangeBounds;
use core::option::IntoIter;

/// A "unique map" that relates a `K` to a `RowPointer`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UniqueMap {
    map: Vec<Vec<RowPointer>>,
}

impl MemoryUsage for UniqueMap {
    fn heap_usage(&self) -> usize {
        let Self { map } = self;
        map.heap_usage()
    }
}

type K = u32;

const NONE_PTR: RowPointer = RowPointer::new(true, PageIndex(0), PageOffset(0), SquashedOffset::TX_STATE);

const KEYS_PER_INNER: usize = 4_096 / size_of::<RowPointer>();

fn split_key(key: K) -> (usize, usize) {
    const N: K = KEYS_PER_INNER as K;
    let (k1, k2) = (key / N, key % N);
    (k1 as usize, k2 as usize)
}

impl UniqueMap {
    /// Inserts the relation `key -> val` to this map.
    ///
    /// If `key` was already present in the map, does not add an association with `val`.
    /// Returns the existing associated value instead.
    pub fn insert(&mut self, key: K, val: RowPointer) -> Result<(), &RowPointer> {
        let (k1, k2) = split_key(key);

        let outer = &mut self.map;
        outer.resize(outer.len().max(k1 + 1), Vec::new());

        // SAFETY: ensured in `.resize(_)` that `k1 < inner.len()`, making indexing to `k1` valid.
        let inner = unsafe { outer.get_unchecked_mut(k1) };
        inner.resize(KEYS_PER_INNER, NONE_PTR);

        // SAFETY: ensured in `.resize(_)` that `inner.len() = KEYS_PER_INNER`,
        // and `k2 = key % KEYS_PER_INNER`, so `k2 < KEYS_PER_INNER`,
        // making indexing to `k2` valid.
        let slot = unsafe { inner.get_unchecked_mut(k2) };

        if slot.reserved_bit() {
            // We have `NONE_PTR`, so not set yet.
            *slot = val;
            Ok(())
        } else {
            Err(slot)
        }
    }

    /// Deletes `key` from this map.
    ///
    /// Returns whether `key` was present.
    pub fn delete(&mut self, key: &K) -> bool {
        let (k1, k2) = split_key(*key);
        let outer = &mut self.map;
        if let Some(inner) = outer.get_mut(k1) {
            if let Some(slot) = inner.get_mut(k2) {
                *slot = NONE_PTR;
                return true;
            }
        }
        false
    }

    /// Returns an iterator over the map that yields all the `V`s
    /// of the `K`s that fall within the specified `range`.
    pub fn values_in_range(&self, range: &impl RangeBounds<K>) -> UniqueMapIter<'_, RowPointer> {
        let Bound::Included(key) = range.start_bound() else {
            unreachable!();
        };

        let (k1, k2) = split_key(*key);
        let outer = &self.map;
        let iter = outer.get(k1).and_then(|inner| inner.get(k2)).into_iter();

        UniqueMapIter { iter }
    }

    /// Returns the number of unique keys in the map.
    pub fn num_keys(&self) -> usize {
        self.len()
    }

    /// Returns the total number of entries in the map.s
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns whether there are any entries in the map.
    #[allow(unused)] // No use for this currently.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Deletes all entries from the map, leaving it empty.
    /// This will not deallocate the outer map.
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

/// An iterator over value in [`UniqueMap`] where the key matches exactly.
pub struct UniqueMapIter<'a, V> {
    /// The iterator seeking for the matching key.
    iter: IntoIter<&'a V>,
}

impl<'a, V> Iterator for UniqueMapIter<'a, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
