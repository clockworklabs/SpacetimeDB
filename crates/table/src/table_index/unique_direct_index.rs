use super::index::{Despecialize, Index, RangedIndex};
use super::{BtreeUniqueIndex, KeySize};
use crate::indexes::{PageIndex, PageOffset, RowPointer, SquashedOffset};
use core::marker::PhantomData;
use core::mem;
use core::ops::{Bound, RangeBounds};
use core::option::IntoIter;
use spacetimedb_sats::memory_usage::MemoryUsage;
use spacetimedb_sats::sum_value::SumTag;

pub trait ToFromUsize: Copy {
    /// Converts value to `usize`.
    fn to_usize(self) -> usize;

    /// Converts `value` to `Self`.
    fn from_usize(x: usize) -> Self;
}

macro_rules! impl_to_from_usize {
    ($ty:ty) => {
        impl ToFromUsize for $ty {
            #[inline]
            fn to_usize(self) -> usize {
                self as usize
            }

            #[inline]
            fn from_usize(x: usize) -> Self {
                x as Self
            }
        }
    };
}

impl_to_from_usize!(u8);
impl_to_from_usize!(u16);
impl_to_from_usize!(u32);
impl_to_from_usize!(u64);
impl_to_from_usize!(usize);

impl ToFromUsize for SumTag {
    #[inline]
    fn to_usize(self) -> usize {
        self.0.to_usize()
    }

    #[inline]
    fn from_usize(x: usize) -> Self {
        Self(u8::from_usize(x))
    }
}

/// A direct index for relating unsigned integer keys [`u8`..`u64`] to [`RowPointer`].
///
/// This index is efficient when given keys that are used in non-random insert patterns
/// where keys are dense and not far apart as well as starting near zero.
/// Conversely, it performs worse than a btree index in the case of highly random inserts
/// and with sparse keys and where the first key inserted is large.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueDirectIndex<K> {
    marker: PhantomData<K>,
    /// The outer index.
    outer: Vec<Option<InnerIndex>>,
    /// The number of keys indexed.
    len: usize,
}

impl<K> MemoryUsage for UniqueDirectIndex<K> {
    fn heap_usage(&self) -> usize {
        let Self { marker: _, outer, len } = self;
        outer.heap_usage() + len.heap_usage()
    }
}

impl<K> Default for UniqueDirectIndex<K> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
            outer: Vec::new(),
            len: 0,
        }
    }
}

/// The standard page size on linux x64.
const PAGE_SIZE: usize = 4_096;
/// Number of keys per inner index.
const KEYS_PER_INNER: usize = PAGE_SIZE / size_of::<RowPointer>();
/// The inner index array, which will be heap allocated.
type InnerIndexArray = [RowPointer; KEYS_PER_INNER];

/// An inner index. Either it is empty, or it has `KEYS_PER_INNER` elements.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InnerIndex {
    inner: Box<InnerIndexArray>,
}

impl MemoryUsage for InnerIndex {
    fn heap_usage(&self) -> usize {
        self.inner.heap_usage()
    }
}

/// The sentinel used to represent an empty slot in the index.
/// The reserved bit set to `false` is used to indicate absence.
pub(super) const NONE_PTR: RowPointer = RowPointer::new(false, PageIndex(0), PageOffset(0), SquashedOffset::TX_STATE);

#[derive(Debug)]
struct InnerIndexKey(usize);

/// Splits the `key` into an outer and inner key.
#[inline]
fn split_key(key: usize) -> (usize, InnerIndexKey) {
    (key / KEYS_PER_INNER, InnerIndexKey(key % KEYS_PER_INNER))
}

/// Converts a row poiner into one for inside consumption.
#[inline]
pub(super) fn injest(ptr: RowPointer) -> RowPointer {
    ptr.with_reserved_bit(true)
}

/// Returns a row pointer for outside consumption.
#[inline]
pub(super) fn expose(ptr: RowPointer) -> RowPointer {
    ptr.with_reserved_bit(false)
}

impl InnerIndex {
    fn new() -> Self {
        use std::alloc::{alloc_zeroed, handle_alloc_error, Layout};

        let layout = Layout::new::<InnerIndexArray>();

        // Allocate with `alloc_zeroed` so that the bytes are initially 0, rather than uninit.
        // This is a sound implementation as `0`-init elements == `NONE_PTR`.
        // TODO: use Box::new_zeroed() once stabilized.
        // SAFETY: The layout's size is non-zero.
        let raw: *mut InnerIndexArray = unsafe { alloc_zeroed(layout) }.cast();

        if raw.is_null() {
            handle_alloc_error(layout);
        }

        // SAFETY: We used the global allocator with a layout for `InnerIndexArray`.
        //         and the elements are 0-init by `alloc_zeroed`,
        //         which makes each element a valid `RowPointer` (`u64`).
        let inner = unsafe { Box::from_raw(raw) };

        Self { inner }
    }

    /// Returns the pointer at `key`.
    fn get(&self, key: InnerIndexKey) -> RowPointer {
        // SAFETY: `self.inner.len() = KEYS_PER_INNER` and `key.0 < KEYS_PER_INNER`.
        *unsafe { self.inner.get_unchecked(key.0) }
    }

    /// Returns the mutable slot at `key`.
    fn get_mut(&mut self, key: InnerIndexKey) -> &mut RowPointer {
        // SAFETY: `self.inner.len() = KEYS_PER_INNER` and `key.0 < KEYS_PER_INNER`.
        unsafe { self.inner.get_unchecked_mut(key.0) }
    }
}

impl<K: ToFromUsize + KeySize> Index for UniqueDirectIndex<K> {
    type Key = K;

    fn clone_structure(&self) -> Self {
        Self::default()
    }

    fn insert_maybe_despecialize(
        &mut self,
        key: Self::Key,
        val: RowPointer,
    ) -> Result<Result<(), RowPointer>, Despecialize> {
        let key = key.to_usize();
        if key > const { u32::MAX as usize } {
            // For large `u64`, this can cause OOM.
            //
            // TODO(perf, centril): do not pay for this cost when `key = u8..u32`.
            //
            // TODO(perf, centril): in the future,
            // collect stats for when too many far apart keys are inserted,
            // and despecialize in that case as well.
            return Err(Despecialize);
        }

        let (key_outer, key_inner) = split_key(key);

        // Fetch the outer index and ensure it can house `key_outer`.
        let outer = &mut self.outer;
        outer.resize(outer.len().max(key_outer + 1), None);

        // Fetch the inner index.
        // SAFETY: ensured in `.resize(_)` that `key_outer < outer.len()`, making indexing to `key_outer` valid.
        let inner = unsafe { outer.get_unchecked_mut(key_outer) };
        let inner = inner.get_or_insert_with(InnerIndex::new);

        // Fetch the slot.
        let slot = inner.get_mut(key_inner);
        let in_slot = *slot;
        Ok(if in_slot == NONE_PTR {
            // We have `NONE_PTR`, so not set yet.
            *slot = injest(val);
            self.len += 1;
            Ok(())
        } else {
            Err(expose(in_slot))
        })
    }

    fn delete(&mut self, key: &Self::Key, _: RowPointer) -> bool {
        let key = key.to_usize();
        let (key_outer, key_inner) = split_key(key);
        let outer = &mut self.outer;
        if let Some(Some(inner)) = outer.get_mut(key_outer) {
            let slot = inner.get_mut(key_inner);
            let old_val = mem::replace(slot, NONE_PTR);
            let deleted = old_val != NONE_PTR;
            self.len -= deleted as usize;
            return deleted;
        }
        false
    }

    type PointIter<'a>
        = UniqueDirectIndexPointIter
    where
        Self: 'a;

    fn seek_point(&self, key: &Self::Key) -> Self::PointIter<'_> {
        let key = key.to_usize();
        let (outer_key, inner_key) = split_key(key);
        let point = self
            .outer
            .get(outer_key)
            .and_then(|x| x.as_ref())
            .map(|inner| inner.get(inner_key))
            .filter(|slot| *slot != NONE_PTR);
        UniqueDirectIndexPointIter::new(point)
    }

    fn num_keys(&self) -> usize {
        self.len
    }

    /// Deletes all entries from the index, leaving it empty.
    /// This will not deallocate the outer index.
    fn clear(&mut self) {
        self.outer.clear();
        self.len = 0;
    }

    /// Returns whether `other` can be merged into `self`
    /// with an error containing the element in `self` that caused the violation.
    ///
    /// The closure `ignore` indicates whether a row in `self` should be ignored.
    fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        for (inner_s, inner_o) in self.outer.iter().zip(&other.outer) {
            let (Some(inner_s), Some(inner_o)) = (inner_s, inner_o) else {
                continue;
            };

            for (slot_s, slot_o) in inner_s.inner.iter().zip(inner_o.inner.iter()) {
                let ptr_s = expose(*slot_s);
                if *slot_s != NONE_PTR && *slot_o != NONE_PTR && !ignore(&ptr_s) {
                    // For the same key, we found both slots occupied, so we cannot merge.
                    return Err(ptr_s);
                }
            }
        }

        Ok(())
    }
}

impl<K: ToFromUsize + KeySize> RangedIndex for UniqueDirectIndex<K> {
    type RangeIter<'a>
        = UniqueDirectIndexRangeIter<'a>
    where
        K: 'a;

    /// Returns an iterator yielding all the [`RowPointer`] that correspond to the provided `range`.
    fn seek_range(&self, range: &impl RangeBounds<Self::Key>) -> Self::RangeIter<'_> {
        // The upper bound of possible key.
        // This isn't necessarily the real max key actually present in the index,
        // due to possible deletions.
        let max_key = self.outer.len() * KEYS_PER_INNER;

        // Translate `range` to `start..end`.
        let start = match range.start_bound() {
            Bound::Included(&s) => s.to_usize(),
            Bound::Excluded(&s) => s.to_usize() + 1, // If this wraps, we will clamp to `max_key` later.
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&e) => e.to_usize() + 1, // If this wraps, we will clamp to `max_key` later.
            Bound::Excluded(&e) => e.to_usize(),
            Bound::Unbounded => max_key,
        };

        // Clamp `end` to max possible key in index.
        let end = end.min(max_key);

        // Normalize `start` so that `start <= end`.
        let start = start.min(end);

        UniqueDirectIndexRangeIter {
            outer: &self.outer,
            start,
            end,
        }
    }
}

/// An iterator over the potential value in a [`UniqueDirectMap`] for a given key.
pub struct UniqueDirectIndexPointIter {
    iter: IntoIter<RowPointer>,
}

impl UniqueDirectIndexPointIter {
    pub(super) fn new(point: Option<RowPointer>) -> Self {
        let iter = point.map(expose).into_iter();
        Self { iter }
    }
}

impl Iterator for UniqueDirectIndexPointIter {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// An iterator over a range of keys in a [`UniqueDirectIndex`].
#[derive(Debug, Clone)]
pub struct UniqueDirectIndexRangeIter<'a> {
    outer: &'a [Option<InnerIndex>],
    start: usize,
    end: usize,
}

impl Iterator for UniqueDirectIndexRangeIter<'_> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.start >= self.end {
                // We're at or beyond the end, so we're done.
                return None;
            }

            let (outer_key, inner_key) = split_key(self.start);
            // SAFETY:
            // - `self.start <= self.end <= max_key`
            // - the early exit above ensures that `self.start < max_key`.
            // - `split_key(max_key).0 = self.outer.len()`.
            // - this entails that `outer_key < self.outer.len()`.
            let inner = unsafe { self.outer.get_unchecked(outer_key) };
            let Some(inner) = inner else {
                // Inner index has not been initialized,
                // so the entire inner index is empty.
                // Let's jump to the next inner index.
                self.start += KEYS_PER_INNER;
                continue;
            };
            let ptr = inner.get(inner_key);

            // Advance to next key.
            self.start += 1;

            if ptr != NONE_PTR {
                // The row actually exists, so we've found something to return.
                return Some(expose(ptr));
            }
        }
    }
}

impl<K: KeySize + Ord + ToFromUsize> UniqueDirectIndex<K> {
    /// Convert this Direct index into a B-Tree index.
    pub fn into_btree(&self) -> BtreeUniqueIndex<K> {
        let mut new_index: BtreeUniqueIndex<K> = <_>::default();

        for (key_outer, inner) in self.outer.iter().enumerate() {
            let Some(inner) = inner else {
                continue;
            };

            for (key_inner, &slot) in inner.inner.iter().enumerate() {
                if slot == NONE_PTR {
                    continue;
                }

                let key = key_outer * KEYS_PER_INNER + key_inner;
                let key = K::from_usize(key);
                new_index
                    .insert(key, expose(slot))
                    .expect("insertions from one unique index to another cannot fail")
            }
        }

        new_index
    }
}

#[cfg(test)]
pub(super) mod test {
    use super::*;
    use core::iter::repeat_with;
    use spacetimedb_sats::layout::Size;

    const FIXED_ROW_SIZE: Size = Size(4 * 4);

    pub(crate) fn gen_row_pointers() -> impl Iterator<Item = RowPointer> {
        let mut page_index = PageIndex(0);
        let mut page_offset = PageOffset(0);
        repeat_with(move || {
            if page_offset.0 as usize + FIXED_ROW_SIZE.0 as usize >= PageOffset::PAGE_END.0 as usize {
                // Consumed the page, let's use a new page.
                page_index.0 += 1;
                page_offset = PageOffset(0);
            } else {
                page_offset += FIXED_ROW_SIZE;
            }

            RowPointer::new(false, page_index, page_offset, SquashedOffset::COMMITTED_STATE)
        })
    }

    #[test]
    fn seek_range_gives_back_inserted() {
        let range = (KEYS_PER_INNER - 2)..(KEYS_PER_INNER + 2);
        let (keys, ptrs): (Vec<_>, Vec<_>) = range.clone().zip(gen_row_pointers()).unzip();

        let mut index = UniqueDirectIndex::default();
        for (key, ptr) in keys.iter().zip(&ptrs) {
            index.insert(*key, *ptr).unwrap();
        }
        assert_eq!(index.num_rows(), 4);

        let ptrs_found = index.seek_range(&range).collect::<Vec<_>>();
        assert_eq!(ptrs, ptrs_found);
    }

    #[test]
    fn inserting_again_errors() {
        let range = (KEYS_PER_INNER - 2)..(KEYS_PER_INNER + 2);
        let (keys, ptrs): (Vec<_>, Vec<_>) = range.zip(gen_row_pointers()).unzip();

        let mut index = UniqueDirectIndex::default();
        for (key, ptr) in keys.iter().zip(&ptrs) {
            index.insert(*key, *ptr).unwrap();
        }

        for (key, ptr) in keys.iter().zip(&ptrs) {
            assert_eq!(index.insert(*key, *ptr).unwrap_err(), *ptr)
        }
    }

    #[test]
    fn deleting_allows_reinsertion() {
        let range = (KEYS_PER_INNER - 2)..(KEYS_PER_INNER + 2);
        let (keys, ptrs): (Vec<_>, Vec<_>) = range.zip(gen_row_pointers()).unzip();

        let mut index = UniqueDirectIndex::default();
        for (key, ptr) in keys.iter().zip(&ptrs) {
            index.insert(*key, *ptr).unwrap();
        }
        assert_eq!(index.num_rows(), 4);

        let key = KEYS_PER_INNER + 1;
        let ptr = index.seek_point(&key).next().unwrap();
        assert!(index.delete(&key, ptr));
        assert!(!index.delete(&key, ptr));
        assert_eq!(index.num_rows(), 3);

        index.insert(key, ptr).unwrap();
        assert_eq!(index.num_rows(), 4);
    }
}
