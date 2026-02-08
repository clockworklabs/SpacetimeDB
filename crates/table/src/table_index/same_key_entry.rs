use crate::{indexes::RowPointer, static_assert_size};
use core::{mem, slice};
use smallvec::SmallVec;
use spacetimedb_data_structures::map::{hash_set, HashCollectionExt, HashSet};
use spacetimedb_memory_usage::MemoryUsage;

type Small = SmallVec<[RowPointer; 2]>;
type Large = HashSet<RowPointer>;

/// A supporting type for multimap implementations
/// that handles all the values for the same key,
/// leaving the multimap to only have to care about the keys.
///
/// For performance reasons,
/// this is an enum
/// that deals with a smaller number of values in the first variant
/// and with a larger number in the second variant.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SameKeyEntry {
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
    ///
    /// Invariant: The callers of [`SameKeyEntry::push`] and [`SameKeyEntry::merge_from`]
    /// ensure that this will never contain duplicates.
    Small(Small),

    /// A large number of values.
    ///
    /// Used when the heap size of `Small` would exceed one standard page.
    /// See [`SameKeyEntry::LARGE_AFTER_LEN`] for details.
    ///
    /// Note that using a `HashSet`, with `S = RandomState`,
    /// entails that the iteration order is not deterministic.
    /// This is observed when doing queries against the index.
    Large(Large),
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

    /// Extends `set` with `elems`.
    ///
    // SAFETY: `elems` must not be contained in `set`.
    #[inline]
    unsafe fn extend_unique(set: &mut Large, elems: impl IntoIterator<Item = RowPointer>) {
        for val in elems.into_iter() {
            // SAFETY: caller promised that `small` contains no duplicates.
            unsafe { set.insert_unique_unchecked(val) };
        }
    }

    /// Pushes `val` as an entry for the key.
    ///
    /// This assumes that `val` was previously not recorded.
    /// The structure does not check whether it was previously resident.
    /// As a consequence, the time complexity is `O(k)` amortized.
    ///
    /// # Safety
    ///
    /// - `val` does not occur in `self`.
    pub(super) unsafe fn push(&mut self, val: RowPointer) {
        match self {
            Self::Small(set) if set.len() <= Self::LARGE_AFTER_LEN => {
                // SAFETY: The caller promised that `val` is not in `set`,
                // so this preserves our invariant that `set` is a set.
                set.push(val);
            }
            Self::Small(list) => {
                // Reconstruct into a hash set.
                let mut set = HashSet::with_capacity(list.len() + 1);
                // SAFETY: Before `.push`, `list` was a set and contained no duplicates.
                unsafe { Self::extend_unique(&mut set, mem::take(list)) };

                // Add `val`.
                // SAFETY: Caller promised that `set` did not include `val`.
                unsafe { set.insert_unique_unchecked(val) };

                *self = Self::Large(set);
            }
            Self::Large(set) => {
                // SAFETY: Caller promised that `set` did not include `val`.
                unsafe { set.insert_unique_unchecked(val) };
            }
        }
    }

    /// Merges all values in `src` into `self`,
    /// translating all the former via `by` first.
    ///
    /// # Safety
    ///
    /// - The intersection of `dst` and `by(src)` is empty.
    ///   That is, `self ∩ by(src) = ∅` holds.
    pub(super) unsafe fn merge_from(&mut self, src: Self, mut by: impl FnMut(RowPointer) -> RowPointer) {
        match src {
            Self::Small(mut src) => {
                // Translate the source.
                for ptr in &mut src {
                    *ptr = by(*ptr);
                }

                // Insert into `self`.
                match self {
                    Self::Small(dst) => {
                        // SAFETY: The intersection is empty, so `dst ++ src` is also a set.
                        dst.append(&mut src);
                    }
                    Self::Large(dst) => {
                        for val in src.into_iter() {
                            // SAFETY: The intersection is empty, so the union is also a set.
                            unsafe { dst.insert_unique_unchecked(val) };
                        }
                    }
                }
            }
            Self::Large(src) => {
                // Translate the source.
                let src = src.into_iter().map(by);

                match self {
                    Self::Small(dst) => {
                        // Reconstruct into a hash set with combined size.
                        let mut set = HashSet::with_capacity(dst.len() + src.len());
                        let dst = mem::take(dst).into_iter();
                        // SAFETY: `dst` is a set by `Self`'s invariant and `set` is empty.
                        unsafe { Self::extend_unique(&mut set, dst) };

                        // Merge `src` into `set`.
                        // SAFETY: The intersection is empty, so the union is also a set.
                        unsafe { Self::extend_unique(&mut set, src) };
                    }
                    Self::Large(dst) => {
                        // Merge `src` into `set`.
                        dst.reserve(src.len());
                        // SAFETY: The intersection is empty, so the union is also a set.
                        unsafe { Self::extend_unique(dst, src) };
                    }
                }
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
