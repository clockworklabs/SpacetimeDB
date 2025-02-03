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
    map: Vec<RowPointer>,
}

impl MemoryUsage for UniqueMap {
    fn heap_usage(&self) -> usize {
        let Self { map } = self;
        map.heap_usage()
    }
}

type K = u32;

const NONE_PTR: RowPointer = RowPointer::new(true, PageIndex(0), PageOffset(0), SquashedOffset::TX_STATE);

impl UniqueMap {
    /// Inserts the relation `key -> val` to this map.
    ///
    /// If `key` was already present in the map, does not add an association with `val`.
    /// Returns the existing associated value instead.
    pub fn insert(&mut self, key: K, val: RowPointer) -> Result<(), &RowPointer> {
        let key = key as usize;

        let after_len = self.map.len().max(key + 1);
        self.map.resize(after_len, NONE_PTR);

        // SAFETY: we just ensured in `.resize(_)` that `key < self.0.len()`,
        // which makes indexing to `key` valid.
        let slot = unsafe { self.map.get_unchecked_mut(key) };

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
        if let Some(slot) = self.map.get_mut(*key as usize) {
            *slot = NONE_PTR;
            true
        } else {
            false
        }
    }

    /// Returns an iterator over the map that yields all the `V`s
    /// of the `K`s that fall within the specified `range`.
    pub fn values_in_range(&self, range: &impl RangeBounds<K>) -> UniqueMapIter<'_, RowPointer> {
        let Bound::Included(key) = range.start_bound() else {
            unreachable!();
        };

        let iter = self.map.get(*key as usize).into_iter();

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
