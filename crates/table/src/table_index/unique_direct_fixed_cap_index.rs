use super::index::{Index, RangedIndex};
use super::unique_direct_index::{UniqueDirectIndexPointIter, NONE_PTR};
use crate::indexes::RowPointer;
use core::mem;
use core::ops::{Bound, RangeBounds};
use core::slice::Iter;
use spacetimedb_sats::memory_usage::MemoryUsage;

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
}

impl Index for UniqueDirectFixedCapIndex {
    type Key = u8;

    /// Clones the structure of the index and returns one with the same capacity.
    fn clone_structure(&self) -> Self {
        Self::new(self.array.len())
    }

    /// Inserts the relation `key -> val` to this index.
    ///
    /// If `key` was already present in the index, does not add an association with `val`.
    /// Returns the existing associated value instead.
    ///
    /// Panics if the key is beyond the fixed capacity of this index.
    fn insert(&mut self, key: Self::Key, val: RowPointer) -> Result<(), RowPointer> {
        // Fetch the slot.
        let slot = &mut self.array[key as usize];
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

    fn delete(&mut self, &key: &Self::Key, _: RowPointer) -> bool {
        let Some(slot) = self.array.get_mut(key as usize) else {
            return false;
        };
        let old_val = mem::replace(slot, NONE_PTR);
        let deleted = old_val != NONE_PTR;
        self.len -= deleted as usize;
        deleted
    }

    type PointIter<'a>
        = UniqueDirectIndexPointIter
    where
        Self: 'a;

    fn seek_point(&self, &key: &Self::Key) -> Self::PointIter<'_> {
        let point = self.array.get(key as usize).copied().filter(|slot| *slot != NONE_PTR);
        UniqueDirectIndexPointIter::new(point)
    }

    fn num_keys(&self) -> usize {
        self.len
    }

    fn clear(&mut self) {
        self.array.fill(NONE_PTR);
        self.len = 0;
    }

    fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
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

impl RangedIndex for UniqueDirectFixedCapIndex {
    type RangeIter<'a>
        = UniqueDirectFixedCapIndexRangeIter<'a>
    where
        Self: 'a;

    fn seek_range(&self, range: &impl RangeBounds<Self::Key>) -> Self::RangeIter<'_> {
        // Translate `range` to `start..end`.
        let end = match range.end_bound() {
            Bound::Included(&e) => e as usize + 1,
            Bound::Excluded(&e) => e as usize,
            Bound::Unbounded => self.array.len(),
        };
        let start = match range.start_bound() {
            Bound::Included(&s) => s as usize,
            Bound::Excluded(&s) => s as usize + 1,
            Bound::Unbounded => 0,
        };

        // Normalize `start` so that `start <= end`.
        let start = start.min(end);

        // Make the iterator.
        UniqueDirectFixedCapIndexRangeIter::new(self.array.get(start..end).unwrap_or_default())
    }
}

/// An iterator over a range of keys in a [`UniqueDirectFixedCapIndex`].
#[derive(Debug, Clone)]
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

    fn range(start: u8, end: u8) -> Range<u8> {
        let min = start.min(end);
        let max = start.max(end);
        min..max
    }

    fn setup(start: u8, end: u8) -> (UniqueDirectFixedCapIndex, Range<u8>, Vec<RowPointer>) {
        let range = range(start, end);
        let (keys, ptrs): (Vec<_>, Vec<_>) = range.clone().zip(gen_row_pointers()).unzip();

        let mut index = UniqueDirectFixedCapIndex::new(u8::MAX as usize + 1);
        for (key, ptr) in keys.iter().zip(&ptrs) {
            index.insert(*key, *ptr).unwrap();
        }
        assert_eq!(index.num_rows(), (range.end - range.start) as usize);
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

            let key = key.clamp(range.start, range.end.saturating_sub(1));

            let ptr = index.seek_point(&key).next().unwrap();
            assert!(index.delete(&key, ptr));
            assert!(!index.delete(&key, ptr));
            assert_eq!(index.num_rows(), (range.end - range.start - 1) as usize);

            index.insert(key, ptr).unwrap();
            assert_eq!(index.num_rows(), (range.end - range.start) as usize);
        }
    }
}
