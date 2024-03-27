use super::polyfill::*;
use super::borrow::DormantMutRef;
use super::node::{marker, Handle, NodeRef};
use super::BTreeMap;
use core::marker::PhantomData;
use core::mem;
use Entry::*;

/// A view into a single entry in a map, which may either be vacant or occupied.
///
/// This `enum` is constructed from the [`entry`] method on [`BTreeMap`].
///
/// [`entry`]: BTreeMap::entry
pub enum Entry<'a, K: 'a, V: 'a, A: Allocator + Clone = Global> {
    /// A vacant entry.
    Vacant(VacantEntry<'a, K, V, A>),

    /// An occupied entry.
    Occupied(OccupiedEntry<'a, K, V>),
}

/// A view into a vacant entry in a `BTreeMap`.
/// It is part of the [`Entry`] enum.
pub struct VacantEntry<'a, K, V, A: Allocator + Clone = Global> {
    pub(super) key: K,
    /// `None` for a (empty) map without root
    pub(super) handle: Option<Handle<NodeRef<marker::Mut<'a>, K, V, marker::Leaf>, marker::Edge>>,
    pub(super) dormant_map: DormantMutRef<'a, BTreeMap<K, V, A>>,

    /// The BTreeMap will outlive this IntoIter so we don't care about drop order for `alloc`.
    pub(super) alloc: A,

    // Be invariant in `K` and `V`
    pub(super) _marker: PhantomData<&'a mut (K, V)>,
}

/// A view into an occupied entry in a `BTreeMap`.
/// It is part of the [`Entry`] enum.
pub struct OccupiedEntry<'a, K, V> {
    pub(super) handle: Handle<NodeRef<marker::Mut<'a>, K, V, marker::LeafOrInternal>, marker::KV>,

    // Be invariant in `K` and `V`
    pub(super) _marker: PhantomData<&'a mut (K, V)>,
}

impl<'a, K, V: Default, A: Allocator + Clone> Entry<'a, K, V, A> {
    /// Ensures a value is in the entry by inserting the default value if empty,
    /// and returns a mutable reference to the value in the entry.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// let mut map: BTreeMap<&str, Option<usize>> = BTreeMap::new();
    /// map.entry("poneyland").or_default();
    ///
    /// assert_eq!(map["poneyland"], None);
    /// ```
    pub fn or_default(self) -> &'a mut V {
        match self {
            Occupied(entry) => entry.into_mut(),
            Vacant(entry) => entry.insert(Default::default()),
        }
    }
}

impl<'a, K, V, A: Allocator + Clone> VacantEntry<'a, K, V, A> {
    /// Sets the value of the entry with the `VacantEntry`'s key,
    /// and returns a mutable reference to it.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::collections::btree_map::Entry;
    ///
    /// let mut map: BTreeMap<&str, u32> = BTreeMap::new();
    ///
    /// if let Entry::Vacant(o) = map.entry("poneyland") {
    ///     o.insert(37);
    /// }
    /// assert_eq!(map["poneyland"], 37);
    /// ```
    pub fn insert(mut self, value: V) -> &'a mut V {
        let out_ptr = match self.handle {
            None => {
                // SAFETY: There is no tree yet so no reference to it exists.
                let map = unsafe { self.dormant_map.awaken() };
                let mut root = NodeRef::new_leaf(self.alloc.clone());
                let val_ptr = root.borrow_mut().push(self.key, value);
                map.root = Some(root.forget_type());
                map.length = 1;
                val_ptr
            }
            Some(handle) => {
                let new_handle = handle.insert_recursing(self.key, value, self.alloc.clone(), |ins| {
                    drop(ins.left);
                    // SAFETY: Pushing a new root node doesn't invalidate
                    // handles to existing nodes.
                    let map = unsafe { self.dormant_map.reborrow() };
                    let root = map.root.as_mut().unwrap(); // same as ins.left
                    root.push_internal_level(self.alloc).push(ins.kv.0, ins.kv.1, ins.right)
                });

                // Get the pointer to the value
                let val_ptr = new_handle.into_val_mut();

                // SAFETY: We have consumed self.handle.
                let map = unsafe { self.dormant_map.awaken() };
                map.length += 1;
                val_ptr
            }
        };

        // Now that we have finished growing the tree using borrowed references,
        // dereference the pointer to a part of it, that we picked up along the way.
        unsafe { &mut *out_ptr }
    }
}

impl<'a, K, V> OccupiedEntry<'a, K, V> {
    /// Gets a mutable reference to the value in the entry.
    ///
    /// If you need a reference to the `OccupiedEntry` that may outlive the
    /// destruction of the `Entry` value, see [`into_mut`].
    ///
    /// [`into_mut`]: OccupiedEntry::into_mut
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::collections::btree_map::Entry;
    ///
    /// let mut map: BTreeMap<&str, usize> = BTreeMap::new();
    /// map.entry("poneyland").or_insert(12);
    ///
    /// assert_eq!(map["poneyland"], 12);
    /// if let Entry::Occupied(mut o) = map.entry("poneyland") {
    ///     *o.get_mut() += 10;
    ///     assert_eq!(*o.get(), 22);
    ///
    ///     // We can use the same Entry multiple times.
    ///     *o.get_mut() += 2;
    /// }
    /// assert_eq!(map["poneyland"], 24);
    /// ```
    pub fn get_mut(&mut self) -> &mut V {
        self.handle.kv_mut().1
    }

    /// Converts the entry into a mutable reference to its value.
    ///
    /// If you need multiple references to the `OccupiedEntry`, see [`get_mut`].
    ///
    /// [`get_mut`]: OccupiedEntry::get_mut
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::collections::btree_map::Entry;
    ///
    /// let mut map: BTreeMap<&str, usize> = BTreeMap::new();
    /// map.entry("poneyland").or_insert(12);
    ///
    /// assert_eq!(map["poneyland"], 12);
    /// if let Entry::Occupied(o) = map.entry("poneyland") {
    ///     *o.into_mut() += 10;
    /// }
    /// assert_eq!(map["poneyland"], 22);
    /// ```
    #[must_use = "`self` will be dropped if the result is not used"]
    pub fn into_mut(self) -> &'a mut V {
        self.handle.into_val_mut()
    }

    /// Sets the value of the entry with the `OccupiedEntry`'s key,
    /// and returns the entry's old value.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::collections::btree_map::Entry;
    ///
    /// let mut map: BTreeMap<&str, usize> = BTreeMap::new();
    /// map.entry("poneyland").or_insert(12);
    ///
    /// if let Entry::Occupied(mut o) = map.entry("poneyland") {
    ///     assert_eq!(o.insert(15), 12);
    /// }
    /// assert_eq!(map["poneyland"], 15);
    /// ```
    pub fn insert(&mut self, value: V) -> V {
        mem::replace(self.get_mut(), value)
    }
}
