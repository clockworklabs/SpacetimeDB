use crate::{indexes::RowPointer, static_assert_size};
use core::slice;
use smallvec::SmallVec;
use spacetimedb_data_structures::map::{hash_set, HashCollectionExt, HashSet};
use spacetimedb_memory_usage::MemoryUsage;

/// A supporting type for multimap implementations
/// that handles all the values for the same key,
/// leaving the multimap to only have to care about the keys.
///
/// For performance reasons,
/// this is an enum
/// that deals with a smaller number of values in the first variant
/// and with a larger number in the second variant.
#[derive(Debug, PartialEq, Eq)]
pub enum SameKeyEntry {
    /// A small number of values.
    ///
    /// No ordering is kept between values.
    /// This makes insertions into amortized `O(k)`
    /// whereas deletions become `O(|values|)` instead.
    /// This is acceptable as `|values|` is small
    /// and because deleting from an array list is `O(n)` either way.
    ///
    /// This also represents the "no values" case,
    /// although the multimap may want to delete the key in that case.
    ///
    /// Up to two values are represented inline here.
    /// It's not profitable to represent this as a separate variant
    /// as that would increase `size_of::<SameKeyEntry>()` by 8 bytes.
    Small(SmallVec<[RowPointer; 2]>),

    /// A large number of values.
    ///
    /// Used when the heap size of `Small` would exceed one standard page.
    /// See [`SameKeyEntry::LARGE_AFTER_LEN`] for details.
    ///
    /// Note that using a `HashSet`, with `S = RandomState`,
    /// entails that the iteration order is not deterministic.
    /// This is observed when doing queries against the index.
    Large(HashSet<RowPointer>),
}

static_assert_size!(SameKeyEntry, 32);

impl Default for SameKeyEntry {
    fn default() -> Self {
        Self::Small(<_>::default())
    }
}

impl MemoryUsage for SameKeyEntry {
    fn heap_usage(&self) -> usize {
        match self {
            Self::Small(x) => x.heap_usage(),
            Self::Large(x) => x.heap_usage(),
        }
    }
}

impl SameKeyEntry {
    /// The number of elements
    /// beyond which the strategy is changed from small to large storage.
    const LARGE_AFTER_LEN: usize = 4096 / size_of::<RowPointer>();

    /// Pushes `val` as an entry for the key.
    ///
    /// This assumes that `val` was previously not recorded.
    /// The structure does not check whether it was previously resident.
    /// As a consequence, the time complexity is `O(k)` amortized.
    pub(super) fn push(&mut self, val: RowPointer) {
        match self {
            Self::Small(list) if list.len() <= Self::LARGE_AFTER_LEN => {
                list.push(val);
            }
            Self::Small(list) => {
                // Reconstruct into a set.
                let mut set = HashSet::with_capacity(list.len() + 1);
                set.extend(list.drain(..));

                // Add `val`.
                set.insert(val);

                *self = Self::Large(set);
            }
            Self::Large(set) => {
                set.insert(val);
            }
        }
    }

    /// Deletes `val` as an entry for the key.
    ///
    /// Returns `(was_deleted, is_empty)`.
    pub(super) fn delete(&mut self, val: RowPointer) -> (bool, bool) {
        match self {
            Self::Small(list) => {
                // The `list` is not sorted, so we have to do a linear scan first.
                if let Some(idx) = list.iter().position(|v| *v == val) {
                    list.swap_remove(idx);
                    (true, list.is_empty())
                } else {
                    (false, false)
                }
            }
            Self::Large(set) => {
                let removed = set.remove(&val);
                let empty = set.is_empty();
                (removed, empty)
            }
        }
    }

    /// Returns an iterator over all the entries for this key.
    pub(super) fn iter(&self) -> SameKeyEntryIter<'_> {
        match self {
            Self::Small(list) => SameKeyEntryIter::Small(list.iter()),
            Self::Large(set) => SameKeyEntryIter::Large(set.iter().into()),
        }
    }

    /// Returns an iterator over no entries.
    pub(super) fn empty_iter<'a>() -> SameKeyEntryIter<'a> {
        SameKeyEntryIter::Small(const { &[] }.iter())
    }
}

/// Returns an iterator for a key's entries `ske`.
/// This efficiently handles the case where there's no key (`None`).
pub(super) fn same_key_iter(ske: Option<&SameKeyEntry>) -> SameKeyEntryIter<'_> {
    match ske {
        None => SameKeyEntry::empty_iter(),
        Some(ske) => ske.iter(),
    }
}

/// An iterator over values in a [`SameKeyEntry`].
#[derive(Clone)]
pub enum SameKeyEntryIter<'a> {
    Small(slice::Iter<'a, RowPointer>),
    /// This variant doesn't occur so much
    /// and we'd like to reduce the footprint of `SameKeyEntryIter`.
    Large(Box<hash_set::Iter<'a, RowPointer>>),
}

static_assert_size!(SameKeyEntryIter, 16);

impl Iterator for SameKeyEntryIter<'_> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Small(list) => list.next(),
            Self::Large(set) => set.next(),
        }
        .copied()
    }
}

/// An iterator over many [`SameKeyEntry`]s.
#[derive(Clone)]
pub struct ManySameKeyEntryIter<'a, OuterIter> {
    /// The outer iterator providing [`SameKeyEntry`]s.
    outer: OuterIter,
    /// The inner iterator for the value set for a found key.
    inner: SameKeyEntryIter<'a>,
}

impl<OuterIter> ManySameKeyEntryIter<'_, OuterIter> {
    /// Returns an iterator drawing entries from `outer`.
    pub fn new(outer: OuterIter) -> Self {
        let inner = SameKeyEntry::empty_iter();
        Self { outer, inner }
    }
}

impl<'a, OI: Iterator<Item = &'a SameKeyEntry>> Iterator for ManySameKeyEntryIter<'a, OI> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // While the inner iterator has elements, yield them.
            if let Some(val) = self.inner.next() {
                return Some(val);
            }
            // Advance and get a new inner, if possible, or quit.
            // We'll come back and yield elements from it in the next iteration.
            let inner = self.outer.next()?;
            self.inner = inner.iter();
        }
    }
}
