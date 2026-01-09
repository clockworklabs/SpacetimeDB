use crate::{indexes::RowPointer, static_assert_size};
use core::hash::Hash;
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
pub(super) enum SameKeyEntry<V: Eq + Hash> {
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
    Small(SmallVec<[V; 2]>),

    /// A large number of values.
    ///
    /// Used when the heap size of `Small` would exceed one standard page.
    /// See [`SameKeyEntry::LARGE_AFTER_LEN`] for details.
    ///
    /// Note that using a `HashSet`, with `S = RandomState`,
    /// entails that the iteration order is not deterministic.
    /// This is observed when doing queries against the index.
    Large(HashSet<V>),
}

static_assert_size!(SameKeyEntry<RowPointer>, 32);

impl<V: Eq + Hash> Default for SameKeyEntry<V> {
    fn default() -> Self {
        Self::Small(<_>::default())
    }
}

impl<V: MemoryUsage + Eq + Hash> MemoryUsage for SameKeyEntry<V> {
    fn heap_usage(&self) -> usize {
        match self {
            Self::Small(x) => x.heap_usage(),
            Self::Large(x) => x.heap_usage(),
        }
    }
}

impl<V: Eq + Hash> SameKeyEntry<V> {
    /// The number of elements
    /// beyond which the strategy is changed from small to large storage.
    const LARGE_AFTER_LEN: usize = 4096 / size_of::<V>();

    /// Pushes `val` as an entry for the key.
    ///
    /// This assumes that `val` was previously not recorded.
    /// The structure does not check whether it was previously resident.
    /// As a consequence, the time complexity is `O(k)` amortized.
    pub(super) fn push(&mut self, val: V) {
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
    pub(super) fn delete(&mut self, val: &V) -> (bool, bool) {
        match self {
            Self::Small(list) => {
                // The `list` is not sorted, so we have to do a linear scan first.
                if let Some(idx) = list.iter().position(|v| v == val) {
                    list.swap_remove(idx);
                    (true, list.is_empty())
                } else {
                    (false, false)
                }
            }
            Self::Large(set) => {
                let removed = set.remove(val);
                let empty = set.is_empty();
                (removed, empty)
            }
        }
    }

    /// Returns an iterator over all the entries for this key.
    pub(super) fn iter(&self) -> SameKeyEntryIter<'_, V> {
        match self {
            Self::Small(list) => SameKeyEntryIter::Small(list.iter()),
            Self::Large(set) => SameKeyEntryIter::Large(set.iter().into()),
        }
    }

    /// Returns an iterator over no entries.
    pub(super) fn empty_iter<'a>() -> SameKeyEntryIter<'a, V> {
        SameKeyEntryIter::Small(const { &[] }.iter())
    }

    /// Returns the number of entries for the same key.
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Small(list) => list.len(),
            Self::Large(set) => set.len(),
        }
    }
}

/// Returns an iterator for a key's entries `ske`.
/// This efficiently handles the case where there's no key (`None`).
pub(super) fn same_key_iter<V: Eq + Hash>(ske: Option<&SameKeyEntry<V>>) -> SameKeyEntryIter<'_, V> {
    match ske {
        None => SameKeyEntry::empty_iter(),
        Some(ske) => ske.iter(),
    }
}

/// An iterator over values in a [`SameKeyEntry`].
pub enum SameKeyEntryIter<'a, V> {
    Small(slice::Iter<'a, V>),
    /// This variant doesn't occur so much
    /// and we'd like to reduce the footprint of `SameKeyEntryIter`.
    Large(Box<hash_set::Iter<'a, V>>),
}

static_assert_size!(SameKeyEntryIter<RowPointer>, 16);

impl<'a, V> Iterator for SameKeyEntryIter<'a, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Small(list) => list.next(),
            Self::Large(set) => set.next(),
        }
    }
}
