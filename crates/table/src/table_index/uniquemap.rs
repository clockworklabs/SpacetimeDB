use super::{Index, KeySize, RangedIndex};
use crate::{indexes::RowPointer, table_index::key_size::KeyBytesStorage};
use core::{ops::RangeBounds, option::IntoIter};
use spacetimedb_sats::memory_usage::MemoryUsage;
use std::collections::btree_map::{BTreeMap, Entry, Range};

/// A "unique map" that relates a `K` to a `RowPointer`.
///
/// (This is just a `BTreeMap<K, RowPointer>`) with a slightly modified interface.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UniqueMap<K: KeySize> {
    /// The map is backed by a `BTreeMap` for relating a key to a value.
    map: BTreeMap<K, RowPointer>,
    /// Storage for [`Index::num_key_bytes`].
    num_key_bytes: K::MemoStorage,
}

impl<K: KeySize> Default for UniqueMap<K> {
    fn default() -> Self {
        Self {
            map: <_>::default(),
            num_key_bytes: <_>::default(),
        }
    }
}

impl<K: KeySize + MemoryUsage> MemoryUsage for UniqueMap<K> {
    fn heap_usage(&self) -> usize {
        let Self { map, num_key_bytes } = self;
        map.heap_usage() + num_key_bytes.heap_usage()
    }
}

impl<K: Ord + KeySize> Index for UniqueMap<K> {
    type Key = K;

    fn clone_structure(&self) -> Self {
        Self::default()
    }

    fn insert(&mut self, key: K, val: RowPointer) -> Result<(), RowPointer> {
        match self.map.entry(key) {
            Entry::Vacant(e) => {
                self.num_key_bytes.add_to_key_bytes::<Self>(e.key());
                e.insert(val);
                Ok(())
            }
            Entry::Occupied(e) => Err(*e.into_mut()),
        }
    }

    fn delete(&mut self, key: &K, _: RowPointer) -> bool {
        let ret = self.map.remove(key).is_some();
        if ret {
            self.num_key_bytes.sub_from_key_bytes::<Self>(key);
        }
        ret
    }

    fn num_keys(&self) -> usize {
        self.map.len()
    }

    fn num_key_bytes(&self) -> u64 {
        self.num_key_bytes.get(self)
    }

    type PointIter<'a>
        = UniqueMapPointIter<'a>
    where
        Self: 'a;

    fn seek_point(&self, key: &Self::Key) -> Self::PointIter<'_> {
        let iter = self.map.get(key).into_iter();
        UniqueMapPointIter { iter }
    }

    /// Deletes all entries from the map, leaving it empty.
    ///
    /// Unfortunately, this will drop the existing allocation.
    fn clear(&mut self) {
        self.map.clear();
        self.num_key_bytes.reset_to_zero();
    }

    fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        let Some(found) = other
            .map
            .keys()
            .find_map(|key| self.map.get(key).filter(|val| !ignore(val)))
        else {
            return Ok(());
        };
        Err(*found)
    }
}

/// An iterator over the potential value in a [`UniqueMap`] for a given key.
pub struct UniqueMapPointIter<'a> {
    /// The iterator seeking for matching keys in the range.
    pub(super) iter: IntoIter<&'a RowPointer>,
}

impl<'a> Iterator for UniqueMapPointIter<'a> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().copied()
    }
}

impl<K: Ord + KeySize> RangedIndex for UniqueMap<K> {
    type RangeIter<'a>
        = UniqueMapRangeIter<'a, K>
    where
        Self: 'a;

    fn seek_range(&self, range: &impl RangeBounds<Self::Key>) -> Self::RangeIter<'_> {
        UniqueMapRangeIter {
            iter: self.map.range((range.start_bound(), range.end_bound())),
        }
    }
}

/// An iterator over values in a [`UniqueMap`] where the keys are in a certain range.
#[derive(Clone)]
pub struct UniqueMapRangeIter<'a, K> {
    /// The iterator seeking for matching keys in the range.
    iter: Range<'a, K, RowPointer>,
}

impl<'a, K> Iterator for UniqueMapRangeIter<'a, K> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(_, v)| *v)
    }
}
