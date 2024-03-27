use super::polyfill::*;
use super::node::{marker, ForceResult::*, Handle, NodeRef};
use super::search::SearchBound;
use core::cmp::Ordering;
use core::hint;
use core::ptr;

// `front` and `back` are always both `None` or both `Some`.
pub struct LeafRange<BorrowType, K, V> {
    front: Option<Handle<NodeRef<BorrowType, K, V, marker::Leaf>, marker::Edge>>,
    back: Option<Handle<NodeRef<BorrowType, K, V, marker::Leaf>, marker::Edge>>,
}

impl<'a, K: 'a, V: 'a> Clone for LeafRange<marker::Immut<'a>, K, V> {
    fn clone(&self) -> Self {
        LeafRange {
            front: self.front.clone(),
            back: self.back.clone(),
        }
    }
}

impl<BorrowType, K, V> LeafRange<BorrowType, K, V> {
    pub fn none() -> Self {
        LeafRange {
            front: None,
            back: None,
        }
    }

    fn is_empty(&self) -> bool {
        self.front == self.back
    }
}

impl<'a, K, V> LeafRange<marker::Immut<'a>, K, V> {
    #[inline]
    pub fn next_checked(&mut self) -> Option<(&'a K, &'a V)> {
        self.perform_next_checked(|kv| kv.into_kv())
    }
}

impl<BorrowType: marker::BorrowType, K, V> LeafRange<BorrowType, K, V> {
    /// If possible, extract some result from the following KV and move to the edge beyond it.
    fn perform_next_checked<F, R>(&mut self, f: F) -> Option<R>
    where
        F: Fn(&Handle<NodeRef<BorrowType, K, V, marker::LeafOrInternal>, marker::KV>) -> R,
    {
        if self.is_empty() {
            None
        } else {
            super::mem::replace(self.front.as_mut().unwrap(), |front| {
                let kv = front.next_kv().ok().unwrap();
                let result = f(&kv);
                (kv.next_leaf_edge(), Some(result))
            })
        }
    }
}

enum LazyLeafHandle<BorrowType, K, V> {
    Root(NodeRef<BorrowType, K, V, marker::LeafOrInternal>), // not yet descended
    Edge(Handle<NodeRef<BorrowType, K, V, marker::Leaf>, marker::Edge>),
}

impl<'a, K: 'a, V: 'a> Clone for LazyLeafHandle<marker::Immut<'a>, K, V> {
    fn clone(&self) -> Self {
        match self {
            LazyLeafHandle::Root(root) => LazyLeafHandle::Root(*root),
            LazyLeafHandle::Edge(edge) => LazyLeafHandle::Edge(*edge),
        }
    }
}

pub struct LazyLeafRange<BorrowType, K, V> {
    front: Option<LazyLeafHandle<BorrowType, K, V>>,
}

impl<'a, K: 'a, V: 'a> Clone for LazyLeafRange<marker::Immut<'a>, K, V> {
    fn clone(&self) -> Self {
        LazyLeafRange {
            front: self.front.clone(),
        }
    }
}

impl<BorrowType, K, V> LazyLeafRange<BorrowType, K, V> {
    pub fn none() -> Self {
        LazyLeafRange { front: None }
    }
}

impl<'a, K, V> LazyLeafRange<marker::Immut<'a>, K, V> {
    #[inline]
    pub unsafe fn next_unchecked(&mut self) -> (&'a K, &'a V) {
        unsafe { self.init_front().unwrap().next_unchecked() }
    }
}

impl<K, V> LazyLeafRange<marker::Dying, K, V> {
    fn take_front(&mut self) -> Option<Handle<NodeRef<marker::Dying, K, V, marker::Leaf>, marker::Edge>> {
        match self.front.take()? {
            LazyLeafHandle::Root(root) => Some(root.first_leaf_edge()),
            LazyLeafHandle::Edge(edge) => Some(edge),
        }
    }

    #[inline]
    pub unsafe fn deallocating_next_unchecked<A: Allocator + Clone>(
        &mut self,
        alloc: A,
    ) -> Handle<NodeRef<marker::Dying, K, V, marker::LeafOrInternal>, marker::KV> {
        debug_assert!(self.front.is_some());
        let front = self.init_front().unwrap();
        unsafe { front.deallocating_next_unchecked(alloc) }
    }

    #[inline]
    pub fn deallocating_end<A: Allocator + Clone>(&mut self, alloc: A) {
        if let Some(front) = self.take_front() {
            front.deallocating_end(alloc)
        }
    }
}

impl<BorrowType: marker::BorrowType, K, V> LazyLeafRange<BorrowType, K, V> {
    fn init_front(&mut self) -> Option<&mut Handle<NodeRef<BorrowType, K, V, marker::Leaf>, marker::Edge>> {
        if let Some(LazyLeafHandle::Root(root)) = &self.front {
            self.front = Some(LazyLeafHandle::Edge(unsafe { ptr::read(root) }.first_leaf_edge()));
        }
        match &mut self.front {
            None => None,
            Some(LazyLeafHandle::Edge(edge)) => Some(edge),
            // SAFETY: the code above would have replaced it.
            Some(LazyLeafHandle::Root(_)) => unsafe { hint::unreachable_unchecked() },
        }
    }
}

impl<BorrowType: marker::BorrowType, K, V> NodeRef<BorrowType, K, V, marker::LeafOrInternal> {
    /// Finds the distinct leaf edges delimiting a specified range in a tree.
    ///
    /// If such distinct edges exist, returns them in ascending order, meaning
    /// that a non-zero number of calls to `next_unchecked` on the `front` of
    /// the result and/or calls to `next_back_unchecked` on the `back` of the
    /// result will eventually reach the same edge.
    ///
    /// If there are no such edges, i.e., if the tree contains no key within
    /// the range, returns an empty `front` and `back`.
    ///
    /// # Safety
    /// Unless `BorrowType` is `Immut`, do not use the handles to visit the same
    /// KV twice.
    unsafe fn find_leaf_edges_spanning_range(
        self,
        lower_bound: SearchBound<impl FnMut(&K) -> Ordering>,
        upper_bound: SearchBound<impl FnMut(&K) -> Ordering>,
    ) -> LeafRange<BorrowType, K, V> {
        match self.search_tree_for_bifurcation(lower_bound, upper_bound) {
            Err(_) => LeafRange::none(),
            Ok((node, lower_edge_idx, upper_edge_idx, mut lower_child_bound, mut upper_child_bound)) => {
                let mut lower_edge = unsafe { Handle::new_edge(ptr::read(&node), lower_edge_idx) };
                let mut upper_edge = unsafe { Handle::new_edge(node, upper_edge_idx) };
                loop {
                    match (lower_edge.force(), upper_edge.force()) {
                        (Leaf(f), Leaf(b)) => {
                            return LeafRange {
                                front: Some(f),
                                back: Some(b),
                            }
                        }
                        (Internal(f), Internal(b)) => {
                            (lower_edge, lower_child_bound) = f.descend().find_lower_bound_edge(lower_child_bound);
                            (upper_edge, upper_child_bound) = b.descend().find_upper_bound_edge(upper_child_bound);
                        }
                        _ => unreachable!("BTreeMap has different depths"),
                    }
                }
            }
        }
    }
}

fn full_range<BorrowType: marker::BorrowType, K, V>(
    root1: NodeRef<BorrowType, K, V, marker::LeafOrInternal>,
) -> LazyLeafRange<BorrowType, K, V> {
    LazyLeafRange {
        front: Some(LazyLeafHandle::Root(root1)),
    }
}

impl<'a, K: 'a, V: 'a> NodeRef<marker::Immut<'a>, K, V, marker::LeafOrInternal> {
    /// Finds the pair of leaf edges delimiting a specific range in a tree.
    ///
    /// The result is meaningful only if the tree is ordered by key, like the tree
    /// in a `BTreeMap` is.
    pub fn range_search(
        self,
        lower_bound: SearchBound<impl FnMut(&K) -> Ordering>,
        upper_bound: SearchBound<impl FnMut(&K) -> Ordering>,
    ) -> LeafRange<marker::Immut<'a>, K, V> {
        // SAFETY: our borrow type is immutable.
        unsafe { self.find_leaf_edges_spanning_range(lower_bound, upper_bound) }
    }

    /// Finds the pair of leaf edges delimiting an entire tree.
    pub fn full_range(self) -> LazyLeafRange<marker::Immut<'a>, K, V> {
        full_range(self)
    }
}

impl<K, V> NodeRef<marker::Dying, K, V, marker::LeafOrInternal> {
    /// Splits a unique reference into a pair of leaf edges delimiting the full range of the tree.
    /// The results are non-unique references allowing massively destructive mutation, so must be
    /// used with the utmost care.
    pub fn full_range(self) -> LazyLeafRange<marker::Dying, K, V> {
        full_range(self)
    }
}

impl<BorrowType: marker::BorrowType, K, V> Handle<NodeRef<BorrowType, K, V, marker::Leaf>, marker::Edge> {
    /// Given a leaf edge handle, returns [`Result::Ok`] with a handle to the neighboring KV
    /// on the right side, which is either in the same leaf node or in an ancestor node.
    /// If the leaf edge is the last one in the tree, returns [`Result::Err`] with the root node.
    pub fn next_kv(
        self,
    ) -> Result<
        Handle<NodeRef<BorrowType, K, V, marker::LeafOrInternal>, marker::KV>,
        NodeRef<BorrowType, K, V, marker::LeafOrInternal>,
    > {
        let mut edge = self.forget_node_type();
        loop {
            edge = match edge.right_kv() {
                Ok(kv) => return Ok(kv),
                Err(last_edge) => match last_edge.into_node().ascend() {
                    Ok(parent_edge) => parent_edge.forget_node_type(),
                    Err(root) => return Err(root),
                },
            }
        }
    }
}

impl<K, V> Handle<NodeRef<marker::Dying, K, V, marker::Leaf>, marker::Edge> {
    /// Given a leaf edge handle into a dying tree, returns the next leaf edge
    /// on the right side, and the key-value pair in between, if they exist.
    ///
    /// If the given edge is the last one in a leaf, this method deallocates
    /// the leaf, as well as any ancestor nodes whose last edge was reached.
    /// This implies that if no more key-value pair follows, the entire tree
    /// will have been deallocated and there is nothing left to return.
    ///
    /// # Safety
    /// - The given edge must not have been previously returned by counterpart
    ///   `deallocating_next_back`.
    /// - The returned KV handle is only valid to access the key and value,
    ///   and only valid until the next call to a `deallocating_` method.
    unsafe fn deallocating_next<A: Allocator + Clone>(
        self,
        alloc: A,
    ) -> Option<(
        Self,
        Handle<NodeRef<marker::Dying, K, V, marker::LeafOrInternal>, marker::KV>,
    )> {
        let mut edge = self.forget_node_type();
        loop {
            edge = match edge.right_kv() {
                Ok(kv) => return Some((unsafe { ptr::read(&kv) }.next_leaf_edge(), kv)),
                Err(last_edge) => match unsafe { last_edge.into_node().deallocate_and_ascend(alloc.clone()) } {
                    Some(parent_edge) => parent_edge.forget_node_type(),
                    None => return None,
                },
            }
        }
    }

    /// Deallocates a pile of nodes from the leaf up to the root.
    /// This is the only way to deallocate the remainder of a tree after
    /// `deallocating_next` and `deallocating_next_back` have been nibbling at
    /// both sides of the tree, and have hit the same edge. As it is intended
    /// only to be called when all keys and values have been returned,
    /// no cleanup is done on any of the keys or values.
    fn deallocating_end<A: Allocator + Clone>(self, alloc: A) {
        let mut edge = self.forget_node_type();
        while let Some(parent_edge) = unsafe { edge.into_node().deallocate_and_ascend(alloc.clone()) } {
            edge = parent_edge.forget_node_type();
        }
    }
}

impl<'a, K, V> Handle<NodeRef<marker::Immut<'a>, K, V, marker::Leaf>, marker::Edge> {
    /// Moves the leaf edge handle to the next leaf edge and returns references to the
    /// key and value in between.
    ///
    /// # Safety
    /// There must be another KV in the direction travelled.
    unsafe fn next_unchecked(&mut self) -> (&'a K, &'a V) {
        super::mem::replace(self, |leaf_edge| {
            let kv = leaf_edge.next_kv().ok().unwrap();
            (kv.next_leaf_edge(), kv.into_kv())
        })
    }
}

impl<K, V> Handle<NodeRef<marker::Dying, K, V, marker::Leaf>, marker::Edge> {
    /// Moves the leaf edge handle to the next leaf edge and returns the key and value
    /// in between, deallocating any node left behind while leaving the corresponding
    /// edge in its parent node dangling.
    ///
    /// # Safety
    /// - There must be another KV in the direction travelled.
    /// - That KV was not previously returned by counterpart
    ///   `deallocating_next_back_unchecked` on any copy of the handles
    ///   being used to traverse the tree.
    ///
    /// The only safe way to proceed with the updated handle is to compare it, drop it,
    /// or call this method or counterpart `deallocating_next_back_unchecked` again.
    unsafe fn deallocating_next_unchecked<A: Allocator + Clone>(
        &mut self,
        alloc: A,
    ) -> Handle<NodeRef<marker::Dying, K, V, marker::LeafOrInternal>, marker::KV> {
        super::mem::replace(self, |leaf_edge| unsafe { leaf_edge.deallocating_next(alloc).unwrap() })
    }
}

impl<BorrowType: marker::BorrowType, K, V> NodeRef<BorrowType, K, V, marker::LeafOrInternal> {
    /// Returns the leftmost leaf edge in or underneath a node - in other words, the edge
    /// you need first when navigating forward (or last when navigating backward).
    #[inline]
    pub fn first_leaf_edge(self) -> Handle<NodeRef<BorrowType, K, V, marker::Leaf>, marker::Edge> {
        let mut node = self;
        loop {
            match node.force() {
                Leaf(leaf) => return leaf.first_edge(),
                Internal(internal) => node = internal.first_edge().descend(),
            }
        }
    }
}

impl<BorrowType: marker::BorrowType, K, V> Handle<NodeRef<BorrowType, K, V, marker::LeafOrInternal>, marker::KV> {
    /// Returns the leaf edge closest to a KV for forward navigation.
    pub fn next_leaf_edge(self) -> Handle<NodeRef<BorrowType, K, V, marker::Leaf>, marker::Edge> {
        match self.force() {
            Leaf(leaf_kv) => leaf_kv.right_edge(),
            Internal(internal_kv) => {
                let next_internal_edge = internal_kv.right_edge();
                next_internal_edge.descend().first_leaf_edge()
            }
        }
    }
}
