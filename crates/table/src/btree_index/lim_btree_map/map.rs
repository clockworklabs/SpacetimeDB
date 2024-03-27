use super::polyfill::*;
use super::borrow::DormantMutRef;
pub use super::entry::{Entry, OccupiedEntry, VacantEntry};
use super::navigate::{LazyLeafRange, LeafRange};
use super::node::{marker, Handle, NodeRef, Root};
use super::search::{SearchBound, SearchResult::*};
use core::cmp::Ordering;
use core::marker::PhantomData;
use core::mem::{self, ManuallyDrop};
use core::ops::Bound;
use core::ptr;
use Entry::*;

// A tree in a `BTreeMap` is a tree in the `node` module with additional invariants:
// - Keys must appear in ascending order (according to the key's type).
// - Every non-leaf node contains at least 1 element (has at least 2 children).
// - Every non-root node contains at least MIN_LEN elements.
//
// An empty map is represented either by the absence of a root node or by a
// root node that is an empty leaf.

pub struct BTreeMap<K, V, A: Allocator + Clone = Global> {
    pub(super) root: Option<Root<K, V>>,
    pub(super) length: usize,
    /// `ManuallyDrop` to control drop order (needs to be dropped after all the nodes).
    pub(super) alloc: ManuallyDrop<A>,
    // For dropck; the `Box` avoids making the `Unpin` impl more strict than before
    _marker: PhantomData<std::boxed::Box<(K, V)>>,
}

impl<K, V, A: Allocator + Clone> Drop for BTreeMap<K, V, A> {
    fn drop(&mut self) {
        drop(unsafe { ptr::read(self) }.into_iter())
    }
}

/// An iterator over the entries of a `BTreeMap`.
///
/// This `struct` is created by the [`iter`] method on [`BTreeMap`]. See its
/// documentation for more.
///
/// [`iter`]: BTreeMap::iter
#[must_use = "iterators are lazy and do nothing unless consumed"]
struct Iter<'a, K: 'a, V: 'a> {
    range: LazyLeafRange<marker::Immut<'a>, K, V>,
    length: usize,
}

/// An owning iterator over the entries of a `BTreeMap`.
///
/// This `struct` is created by the [`into_iter`] method on [`BTreeMap`]
/// (provided by the [`IntoIterator`] trait). See its documentation for more.
///
/// [`into_iter`]: IntoIterator::into_iter
/// [`IntoIterator`]: core::iter::IntoIterator
pub struct IntoIter<K, V, A: Allocator + Clone = Global> {
    range: LazyLeafRange<marker::Dying, K, V>,
    length: usize,
    /// The BTreeMap will outlive this IntoIter so we don't care about drop order for `alloc`.
    alloc: A,
}

/// An iterator over the values of a `BTreeMap`.
///
/// This `struct` is created by the [`values`] method on [`BTreeMap`]. See its
/// documentation for more.
///
/// [`values`]: BTreeMap::values
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Values<'a, K, V> {
    inner: Iter<'a, K, V>,
}

/// An iterator over a sub-range of entries in a `BTreeMap`.
///
/// This `struct` is created by the [`range`] method on [`BTreeMap`]. See its
/// documentation for more.
///
/// [`range`]: BTreeMap::range
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Range<'a, K: 'a, V: 'a> {
    inner: LeafRange<marker::Immut<'a>, K, V>,
}

impl<K, V> BTreeMap<K, V> {
    #[must_use]
    pub const fn new() -> BTreeMap<K, V> {
        BTreeMap {
            root: None,
            length: 0,
            alloc: ManuallyDrop::new(Global),
            _marker: PhantomData,
        }
    }
}

impl<K, V, A: Allocator + Clone> BTreeMap<K, V, A> {
    fn into_iter(self) -> IntoIter<K, V, A> {
        let mut me = ManuallyDrop::new(self);
        if let Some(root) = me.root.take() {
            let full_range = root.into_dying().full_range();

            IntoIter {
                range: full_range,
                length: me.length,
                alloc: unsafe { ManuallyDrop::take(&mut me.alloc) },
            }
        } else {
            IntoIter {
                range: LazyLeafRange::none(),
                length: 0,
                alloc: unsafe { ManuallyDrop::take(&mut me.alloc) },
            }
        }
    }

    pub fn clear(&mut self) {
        // avoid moving the allocator
        mem::drop(BTreeMap {
            root: mem::replace(&mut self.root, None),
            length: mem::replace(&mut self.length, 0),
            alloc: self.alloc.clone(),
            _marker: PhantomData,
        });
    }
}

impl<K, V, A: Allocator + Clone> BTreeMap<K, V, A> {
    pub fn get_mut(&mut self, comp: impl FnMut(&K) -> Ordering) -> Option<&mut V> {
        let root_node = self.root.as_mut()?.borrow_mut();
        match root_node.search_tree(comp) {
            Found(handle) => Some(handle.into_val_mut()),
            GoDown(_) => None,
        }
    }

    pub fn range(
        &self,
        lower_bound: Bound<impl FnMut(&K) -> Ordering>,
        upper_bound: Bound<impl FnMut(&K) -> Ordering>,
    ) -> Range<'_, K, V> {
        if let Some(root) = &self.root {
            Range {
                inner: root.reborrow().range_search(
                    SearchBound::from_range(lower_bound),
                    SearchBound::from_range(upper_bound),
                ),
            }
        } else {
            Range {
                inner: LeafRange::none(),
            }
        }
    }

    pub fn entry(&mut self, key: K, mut comp: impl FnMut(&K, &K) -> Ordering) -> Entry<'_, K, V, A> {
        let (map, dormant_map) = DormantMutRef::new(self);
        match map.root {
            None => Vacant(VacantEntry {
                key,
                handle: None,
                dormant_map,
                alloc: (*map.alloc).clone(),
                _marker: PhantomData,
            }),
            Some(ref mut root) => match root.borrow_mut().search_tree(|k| comp(&key, k)) {
                Found(handle) => Occupied(OccupiedEntry {
                    handle,
                    _marker: PhantomData,
                }),
                GoDown(handle) => Vacant(VacantEntry {
                    key,
                    handle: Some(handle),
                    dormant_map,
                    alloc: (*map.alloc).clone(),
                    _marker: PhantomData,
                }),
            },
        }
    }
}

impl<'a, K: 'a, V: 'a> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<(&'a K, &'a V)> {
        if self.length == 0 {
            None
        } else {
            self.length -= 1;
            Some(unsafe { self.range.next_unchecked() })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.length, Some(self.length))
    }
}

impl<K, V, A: Allocator + Clone> Drop for IntoIter<K, V, A> {
    fn drop(&mut self) {
        struct DropGuard<'a, K, V, A: Allocator + Clone>(&'a mut IntoIter<K, V, A>);

        impl<'a, K, V, A: Allocator + Clone> Drop for DropGuard<'a, K, V, A> {
            fn drop(&mut self) {
                // Continue the same loop we perform below. This only runs when unwinding, so we
                // don't have to care about panics this time (they'll abort).
                while let Some(kv) = self.0.dying_next() {
                    // SAFETY: we consume the dying handle immediately.
                    unsafe { kv.drop_key_val() };
                }
            }
        }

        while let Some(kv) = self.dying_next() {
            let guard = DropGuard(self);
            // SAFETY: we don't touch the tree before consuming the dying handle.
            unsafe { kv.drop_key_val() };
            mem::forget(guard);
        }
    }
}

impl<K, V, A: Allocator + Clone> IntoIter<K, V, A> {
    /// Core of a `next` method returning a dying KV handle,
    /// invalidated by further calls to this function and some others.
    fn dying_next(&mut self) -> Option<Handle<NodeRef<marker::Dying, K, V, marker::LeafOrInternal>, marker::KV>> {
        if self.length == 0 {
            self.range.deallocating_end(self.alloc.clone());
            None
        } else {
            self.length -= 1;
            Some(unsafe { self.range.deallocating_next_unchecked(self.alloc.clone()) })
        }
    }
}

impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<&'a V> {
        self.inner.next().map(|(_, v)| v)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, K, V> Iterator for Range<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<(&'a K, &'a V)> {
        self.inner.next_checked()
    }
}

impl<K, V> Default for BTreeMap<K, V> {
    /// Creates an empty `BTreeMap`, ordered by a default `O` order.
    fn default() -> BTreeMap<K, V> {
        BTreeMap::new()
    }
}

impl<K, V, A: Allocator + Clone> BTreeMap<K, V, A> {
    fn iter(&self) -> Iter<'_, K, V> {
        if let Some(root) = &self.root {
            let full_range = root.reborrow().full_range();

            Iter {
                range: full_range,
                length: self.length,
            }
        } else {
            Iter {
                range: LazyLeafRange::none(),
                length: 0,
            }
        }
    }

    pub fn values(&self) -> Values<'_, K, V> {
        Values { inner: self.iter() }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.length
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
