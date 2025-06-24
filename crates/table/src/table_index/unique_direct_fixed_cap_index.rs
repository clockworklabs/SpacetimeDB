use super::unique_direct_index::{UniqueDirectIndexPointIter, NONE_PTR};
use crate::indexes::RowPointer;
use crate::MemoryUsage;
use core::mem;
use core::ops::{Bound, RangeBounds};
use core::slice::Iter;

/// A direct index with for relating unsigned integer keys to [`RowPointer`].
/// The index is provided a capacity on creation and will have that during its lifetime.
///
/// These indices are intended for small fixed capacities
/// and will be efficient for both monotonic and random insert patterns for small capacities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueDirectFixedCapIndex {
    /// The array holding the elements.
    array: Box<[RowPointer]>,
    /// The number of keys indexed.
    len: usize,
}

impl MemoryUsage for UniqueDirectFixedCapIndex {
    fn heap_usage(&self) -> usize {
        let Self { array, len } = self;
        array.heap_usage() + len.heap_usage()
    }
}

impl UniqueDirectFixedCapIndex {
    /// Returns a new fixed capacity index.
    pub fn new(cap: usize) -> Self {
        Self {
            len: 0,
            array: vec![NONE_PTR; cap].into(),
        }
    }

    /// Clones the structure of the index and returns one with the same capacity.
    pub fn clone_structure(&self) -> Self {
        Self::new(self.array.len())
    }

    /// Inserts the relation `key -> val` to this index.
    ///
    /// If `key` was already present in the index, does not add an association with `val`.
    /// Returns the existing associated value instead.
    ///
    /// Panics if the key is beyond the fixed capacity of this index.
    pub fn insert(&mut self, key: usize, val: RowPointer) -> Result<(), RowPointer> {
        // Fetch the slot.
        let slot = &mut self.array[key];
        let in_slot = *slot;
        if in_slot == NONE_PTR {
            // We have `NONE_PTR`, so not set yet.
            *slot = val.with_reserved_bit(true);
            self.len += 1;
            Ok(())
        } else {
            Err(in_slot.with_reserved_bit(false))
        }
    }

    /// Deletes `key` from this map.
    ///
    /// Returns whether `key` was present.
    pub fn delete(&mut self, key: usize) -> bool {
        let Some(slot) = self.array.get_mut(key) else {
            return false;
        };
        let old_val = mem::replace(slot, NONE_PTR);
        let deleted = old_val != NONE_PTR;
        self.len -= deleted as usize;
        deleted
    }

    /// Returns an iterator yielding the potential [`RowPointer`] for `key`.
    pub fn seek_point(&self, key: usize) -> UniqueDirectIndexPointIter {
        let point = self.array.get(key).copied().filter(|slot| *slot != NONE_PTR);
        UniqueDirectIndexPointIter::new(point)
    }

    /// Returns an iterator yielding all the [`RowPointer`] that correspond to the provided `range`.
    pub fn seek_range(&self, range: &impl RangeBounds<usize>) -> UniqueDirectFixedCapIndexRangeIter {
        // Translate `range` to `start..end`.
        let end = match range.end_bound() {
            Bound::Included(&e) => e + 1,
            Bound::Excluded(&e) => e,
            Bound::Unbounded => self.array.len(),
        };
        let start = match range.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s + 1,
            Bound::Unbounded => 0,
        };

        // Normalize `start` so that `start <= end`.
        let start = start.min(end);

        // Make the iterator.
        UniqueDirectFixedCapIndexRangeIter::new(self.array.get(start..end).unwrap_or_default())
    }

    /// Returns the number of unique keys in the index.
    pub fn num_keys(&self) -> usize {
        self.len
    }

    /// Returns the total number of entries in the index.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether there are any entries in the index.
    #[allow(unused)] // No use for this currently.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Deletes all entries from the index, leaving it empty.
    pub fn clear(&mut self) {
        self.array.fill(NONE_PTR);
        self.len = 0;
    }

    /// Returns whether `other` can be merged into `self`
    /// with an error containing the element in `self` that caused the violation.
    ///
    /// The closure `ignore` indicates whether a row in `self` should be ignored.
    pub(crate) fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        for (slot_s, slot_o) in self.array.iter().zip(other.array.iter()) {
            let ptr_s = slot_s.with_reserved_bit(false);
            if *slot_s != NONE_PTR && *slot_o != NONE_PTR && !ignore(&ptr_s) {
                // For the same key, we found both slots occupied, so we cannot merge.
                return Err(ptr_s);
            }
        }
        Ok(())
    }
}

/// An iterator over a range of keys in a [`UniqueDirectFixedCapIndex`].
#[derive(Debug)]
pub struct UniqueDirectFixedCapIndexRangeIter<'a> {
    iter: Iter<'a, RowPointer>,
}

impl<'a> UniqueDirectFixedCapIndexRangeIter<'a> {
    fn new(slice: &'a [RowPointer]) -> Self {
        let iter = slice.iter();
        Self { iter }
    }
}

impl Iterator for UniqueDirectFixedCapIndexRangeIter<'_> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            // Make sure the row exists.
            .find(|slot| **slot != NONE_PTR)
            .map(|ptr| ptr.with_reserved_bit(false))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::table_index::unique_direct_index::test::gen_row_pointers;
    use core::ops::Range;
    use proptest::prelude::*;

    fn range(start: u8, end: u8) -> Range<usize> {
        let min = start.min(end);
        let max = start.max(end);
        min as usize..max as usize
    }

    fn setup(start: u8, end: u8) -> (UniqueDirectFixedCapIndex, Range<usize>, Vec<RowPointer>) {
        let range = range(start, end);
        let (keys, ptrs): (Vec<_>, Vec<_>) = range.clone().zip(gen_row_pointers()).unzip();

        let mut index = UniqueDirectFixedCapIndex::new(u8::MAX as usize + 1);
        for (key, ptr) in keys.iter().zip(&ptrs) {
            index.insert(*key, *ptr).unwrap();
        }
        assert_eq!(index.len(), range.end - range.start);
        (index, range, ptrs)
    }

    proptest! {
        #[test]
        fn seek_range_gives_back_inserted(start: u8, end: u8) {
            let (index, range, ptrs) = setup(start, end);
            let ptrs_found = index.seek_range(&range).collect::<Vec<_>>();
            assert_eq!(ptrs, ptrs_found);
        }

        #[test]
        fn inserting_again_errors(start: u8, end: u8) {
            let (mut index, keys, ptrs) = setup(start, end);
            for (key, ptr) in keys.zip(&ptrs) {
                assert_eq!(index.insert(key, *ptr).unwrap_err(), *ptr)
            }
        }

        #[test]
        fn deleting_allows_reinsertion(start: u8, end: u8, key: u8) {
            let (mut index, range, _) = setup(start, end);

            if range.start == range.end {
                return Err(TestCaseError::Reject("empty range".into()));
            }

            let key = (key as usize).clamp(range.start, range.end.saturating_sub(1));

            let ptr = index.seek_point(key).next().unwrap();
            assert!(index.delete(key));
            assert!(!index.delete(key));
            assert_eq!(index.len(), range.end - range.start - 1);

            index.insert(key, ptr).unwrap();
            assert_eq!(index.len(), range.end - range.start);
        }
    }
}
