//! Provides [`PointerMap`] that deals with the
//! association of a [`RowHash`] to a [`RowPointer`]
//! through operations [`insert`](self::PointerMap::insert)
//! and [`delete`](PointerMap::delete).
//!
//! These associations can then be queried through
//! `map.pointers_for(hash)` and `map.pointers_for_mut(hash)`.
//! In most cases, this will result in a `1:1` mapping
//! and so a direct hit in a hash map.
//! If however multiple pointers collide to a single hash,
//! all of these pointers will be returned, in an arbitrary unstable order.
//! Pointers are returned as a slice, which does not require an allocation.
//! In this highly unlikely event of a collision,
//! retrieval is probably no more than 100% slower.

use super::indexes::{PageIndex, PageOffset, RowHash, RowPointer, SquashedOffset};
use crate::static_assert_size;
use core::{hint, slice};
use nohash_hasher::IntMap; // No need to hash a hash.
use std::collections::hash_map::Entry;

/// An index to the outer layer of `colliders` in `PointerMap`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ColliderSlotIndex(u32);

impl ColliderSlotIndex {
    /// Returns a new slot index based on `idx`.
    fn new(idx: usize) -> Self {
        Self(idx as u32)
    }

    /// Returns the index as a `usize`.
    fn idx(self) -> usize {
        self.0 as usize
    }
}

/// A pointer into the `pages` of a table
/// or, for any `RowHash` collisions in `map`,
/// the index in `colliders` to a list of `RowPointer`s.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct PtrOrCollider(RowPointer);

/// An unpacked representation of [`&mut PtrOrCollider`](PtrOrCollider).
enum MapSlotRef<'map> {
    /// The hash has no collision and is associated to a single row pointer.
    Pointer(&'map RowPointer),
    /// The hash has collisions
    /// and all of the associated row pointers can be found at `map.colliders[idx]`.
    Collider(ColliderSlotIndex),
}

/// An unpacked representation of [`&PtrOrCollider`](PtrOrCollider).
enum MapSlotMut<'map> {
    /// The hash has no collision and is associated to a single row pointer.
    Pointer(&'map mut RowPointer),
    /// The hash has collisions
    /// and all of the associated row pointers can be found at `map.colliders[idx]`.
    Collider(ColliderSlotIndex),
}

/// Ensures `rp` is treated as a `RowPointer` by the map, and not as a collider.
/// This is achieved by setting the reserved bit,
/// used by [`PtrOrCollider::is_ptr`], to `false`.
#[inline]
const fn ensure_ptr(rp: RowPointer) -> RowPointer {
    rp.with_reserved_bit(false)
}

impl PtrOrCollider {
    /// Returns a pointer.
    const fn ptr(rp: RowPointer) -> Self {
        Self(ensure_ptr(rp))
    }

    /// Returns a collider.
    const fn collider(c: ColliderSlotIndex) -> Self {
        // Pack the `ColliderSlotIndex` into the page index bits.
        let pi = PageIndex(c.0 as u64);
        Self(RowPointer::new(
            true,
            pi,
            PageOffset::VAR_LEN_NULL,
            SquashedOffset::COMMITTED_STATE,
        ))
    }

    /// Returns whether this is a pointer or not.
    const fn is_ptr(&self) -> bool {
        !self.0.reserved_bit()
    }

    /// Assumes that `self` is a `ColliderSlotIndex` and returns it as such.
    const fn as_collider(&self) -> ColliderSlotIndex {
        ColliderSlotIndex(self.0.page_index().0 as u32)
    }

    /// Convert the packed representation into an unpacked one.
    const fn unpack(&self) -> MapSlotRef<'_> {
        if self.is_ptr() {
            MapSlotRef::Pointer(&self.0)
        } else {
            MapSlotRef::Collider(self.as_collider())
        }
    }

    /// Convert the packed representation into an unpacked one.
    fn unpack_mut(&mut self) -> MapSlotMut<'_> {
        if self.is_ptr() {
            MapSlotMut::Pointer(&mut self.0)
        } else {
            MapSlotMut::Collider(self.as_collider())
        }
    }
}

impl From<ColliderSlotIndex> for PtrOrCollider {
    fn from(index: ColliderSlotIndex) -> Self {
        Self::collider(index)
    }
}

/// An pointer map `RowHash -> [RowPointer]`.
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct PointerMap {
    /// The pointer map from row hashes to row pointer(s).
    ///
    /// Invariant: `self.maintains_map_invariant()`.
    map: IntMap<RowHash, PtrOrCollider>,
    /// The inner vector is a list ("slot") of row pointers that share a row hash.
    /// The outer is indexed by [`ColliderSlotIndex`].
    ///
    /// This indirect approach is used,
    /// rather than storing a list of [`RowPointer`]s,
    /// to reduce the cost for the more common case (fewer collisions).
    ///
    /// This list is append-only as `ColliderSlotIndex` have to be stable.
    /// When removing a row pointer causes a slot to become empty,
    /// the index is added to `emptied_collider_slots` and it can be reused.
    /// This is done to avoid a linear scan of `colliders` for the first empty slot.
    ///
    /// Invariant: `self.maintains_colliders_invariant()`.
    // TODO(centril,perf): Use a `SatsBuffer<T>` with `len/capacity: u32` to reduce size.
    colliders: Vec<Vec<RowPointer>>,
    /// Stack of emptied collider slots.
    // TODO(centril,perf): Use a `SatsBuffer<T>` with `len/capacity: u32` to reduce size.
    emptied_collider_slots: Vec<ColliderSlotIndex>,
}

static_assert_size!(PointerMap, 80);

// Provides some type invariant checks.
// These are only used as sanity checks in the debug profile, and e.g., in tests.
#[cfg(debug_assertions)]
impl PointerMap {
    fn maintains_invariants(&self) -> bool {
        self.maintains_map_invariant() && self.maintains_colliders_invariant()
    }

    fn maintains_colliders_invariant(&self) -> bool {
        self.colliders.iter().enumerate().all(|(idx, slot)| {
            slot.len() >= 2 || slot.is_empty() && self.emptied_collider_slots.contains(&ColliderSlotIndex::new(idx))
        })
    }

    fn maintains_map_invariant(&self) -> bool {
        self.map.values().all(|poc| {
            let collider = poc.as_collider();
            poc.is_ptr()
                || self.colliders[collider.idx()].len() >= 2 && !self.emptied_collider_slots.contains(&collider)
        })
    }
}

// `debug_assert!` conditions are always typechecked, even when debug assertions are disabled.
// This means that we would see a build error in release mode
// due to `PointerMap::maintains_invariants` being undefined.
// Easily solved by including a stub definition.
#[cfg(not(debug_assertions))]
impl PointerMap {
    fn maintains_invariants(&self) -> bool {
        unreachable!(
            "`PointerMap::maintains_invariants` is only meaningfully defined when building with debug assertions."
        )
    }
}

// Provides the public API.
impl PointerMap {
    /// The number of colliding hashes in the map.
    ///
    /// If two hashes collide then this counts as 2.
    pub fn num_collisions(&self) -> usize {
        self.colliders.iter().map(|a| a.len()).sum()
    }

    /// The number hashes that do not collide.
    pub fn num_non_collisions(&self) -> usize {
        self.map.len() - (self.colliders.len() - self.emptied_collider_slots.len())
    }

    /// The number of pointers in the map. This is equal to the number of non-colliding hashes
    /// plus the number of colliding hashes.
    pub fn len(&self) -> usize {
        self.num_collisions() + self.num_non_collisions()
    }

    /// Returns true if there are no pointers in the map.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the row pointers associated with the given row `hash`.
    pub fn pointers_for(&self, hash: RowHash) -> &[RowPointer] {
        self.map.get(&hash).map_or(&[], |poc| match poc.unpack() {
            MapSlotRef::Pointer(ro) => slice::from_ref(ro),
            MapSlotRef::Collider(ci) => &self.colliders[ci.idx()],
        })
    }

    /// Returns the row pointers associated with the given row `hash`.
    ///
    /// Take care not to change the reserved bit of any row pointer
    /// or this will mess up the internal state of the [`PointerMap`].
    pub fn pointers_for_mut(&mut self, hash: RowHash) -> &mut [RowPointer] {
        self.map.get_mut(&hash).map_or(&mut [], |poc| match poc.unpack_mut() {
            MapSlotMut::Pointer(ro) => slice::from_mut(ro),
            MapSlotMut::Collider(ci) => &mut self.colliders[ci.idx()],
        })
    }

    /// Associates row `hash` with row `ptr`.
    /// Returns whether `hash` was already associated with `ptr`
    ///
    /// Handles any hash conflicts for `hash`.
    pub fn insert(&mut self, hash: RowHash, ptr: RowPointer) -> bool {
        debug_assert!(self.maintains_invariants());

        let mut was_in_map = false;

        self.map
            .entry(hash)
            .and_modify(|v| match v.unpack() {
                // Already in map; bail for idempotence.
                MapSlotRef::Pointer(existing) if *existing == ptr => was_in_map = true,
                // Stored inline => colliders list.
                MapSlotRef::Pointer(existing) => {
                    let ptrs = [*existing, ptr].map(ensure_ptr);
                    let ci = match self.emptied_collider_slots.pop() {
                        // Allocate a new colliders slot.
                        None => {
                            let ci = ColliderSlotIndex::new(self.colliders.len());
                            self.colliders.push(ptrs.into());
                            ci
                        }
                        // Reuse an empty slot.
                        Some(ci) => {
                            self.colliders[ci.idx()].extend(ptrs);
                            ci
                        }
                    };
                    *v = PtrOrCollider::collider(ci);
                }
                // Already using a list; add to it.
                MapSlotRef::Collider(ci) => {
                    let ptr = ensure_ptr(ptr);
                    let colliders = &mut self.colliders[ci.idx()];
                    if colliders.contains(&ptr) {
                        // Already in map; bail for idempotence.
                        //
                        // O(n) check, but that's OK,
                        // as we only regress perf in case we have > 5_000
                        // collisions for this `hash`.
                        //
                        // Let `n` be the number of bits (`64`)
                        // and `k` be the number of hashes.
                        // The average number of collisions, `avg`,
                        // according to the birthday problem is:
                        // `avg = 2^(-n) * combinations(k, 2)`.
                        // (Caveat: our hash function is not truly random.)
                        //
                        // Solving for `avg = 5000`, we get `k ≈ 5 * 10^11`.
                        // That is, we need around half a trillion hashes before,
                        // on average, getting 5_000 collisions.
                        // So we can safely ignore this in terms of perf.
                        return was_in_map = true;
                    }
                    colliders.push(ptr);
                }
            })
            // 0 hashes so far.
            .or_insert(PtrOrCollider::ptr(ptr));

        debug_assert!(self.maintains_invariants());

        was_in_map
    }

    /// Removes the association `hash -> ptr`.
    ///
    /// Returns whether the association was deleted.
    pub fn remove(&mut self, hash: RowHash, ptr: RowPointer) -> bool {
        debug_assert!(self.maintains_invariants());

        let ret = 'fun: {
            let Entry::Occupied(mut entry) = self.map.entry(hash) else {
                break 'fun false;
            };

            match entry.get().unpack() {
                // Remove entry on `hash -> [ptr]`.
                MapSlotRef::Pointer(o) if *o == ptr => drop(entry.remove()),
                MapSlotRef::Pointer(_) => break 'fun false,
                MapSlotRef::Collider(ci) => {
                    // Find `ptr` in slot and remove.
                    let slot = &mut self.colliders[ci.idx()];
                    let Some(idx) = slot.iter().position(|o| *o == ptr) else {
                        break 'fun false;
                    };
                    slot.swap_remove(idx);

                    match slot.len() {
                        // SAFETY: This never happens per `self.maintains_collider_invariant()`.
                        0 => unsafe { hint::unreachable_unchecked() },
                        // Simplify; don't use collider list since `hash -> [a_ptr]`.
                        1 => *entry.get_mut() = PtrOrCollider::ptr(slot.pop().unwrap()),
                        _ => break 'fun true,
                    }

                    // Slot is now empty; reuse later.
                    self.emptied_collider_slots.push(ci);
                }
            }

            true
        };

        debug_assert!(self.maintains_invariants());

        ret
    }
}

impl FromIterator<(RowHash, RowPointer)> for PointerMap {
    fn from_iter<T: IntoIterator<Item = (RowHash, RowPointer)>>(iter: T) -> Self {
        let mut map = PointerMap::default();
        for (h, o) in iter {
            let _ = map.insert(h, o);
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::hash::Hash;
    use core::mem;
    use itertools::Itertools;
    use proptest::collection::vec;
    use proptest::prelude::*;

    type R = Result<(), TestCaseError>;

    fn gen_row_pointer() -> impl Strategy<Value = RowPointer> {
        (any::<PageOffset>(), any::<PageIndex>()).prop_map(|(po, pi)| RowPointer::new(false, pi, po, SquashedOffset(0)))
    }

    fn collect_entries(map: &PointerMap) -> Vec<(RowHash, PtrOrCollider)> {
        map.map.iter().map(|(h, o)| (*h, *o)).collect::<Vec<_>>()
    }

    fn entry(hash: RowHash, ptr: RowPointer) -> (RowHash, PtrOrCollider) {
        (hash, PtrOrCollider(ptr))
    }

    fn sorted<T: Ord + Copy>(xs: &[T]) -> Vec<T> {
        xs.iter().copied().sorted().collect()
    }

    fn assert_ptrs_are(map: &mut PointerMap, hash: RowHash, ptrs: &[RowPointer]) -> R {
        let ptrs = sorted(ptrs);
        prop_assert_eq!(sorted(map.pointers_for(hash)), &*ptrs);
        prop_assert_eq!(sorted(map.pointers_for_mut(hash)), ptrs);
        Ok(())
    }

    fn assert_ptrs_and_len(map: &mut PointerMap, hash: RowHash, ptrs: &[RowPointer]) -> R {
        assert_ptrs_are(map, hash, ptrs)?;
        prop_assert_eq!(map.len(), ptrs.len());
        prop_assert_eq!(map.is_empty(), ptrs.is_empty());
        Ok(())
    }

    fn assert_collisions(map: &PointerMap, num_collisions: usize, num_not: usize) -> R {
        prop_assert_eq!(map.num_collisions(), num_collisions);
        prop_assert_eq!(map.num_non_collisions(), num_not);
        Ok(())
    }

    fn ensure_unique<T: Eq + Hash>(xs: &[T]) -> R {
        if !xs.iter().all_unique() {
            return Err(TestCaseError::reject("all elements must be unique"));
        }
        Ok(())
    }

    proptest! {
        #[test]
        fn insert_same_twice_idempotence(
            (hash, ptrs) in (
                any::<RowHash>(),
                vec(gen_row_pointer(), 3..10)
            )
         ) {
            ensure_unique(&ptrs)?;

            let mut map = PointerMap::default();

            // Test the inline case.
            let ptr = ptrs[0];
            prop_assert_eq!(map.insert(hash, ptr), false);
            let old_map = map.clone(); // Savepoint
            prop_assert_eq!(map.insert(hash, ptr), true); // Insert again.
            prop_assert_eq!(&map, &old_map); // No change!
            // Check API state:
            assert_ptrs_and_len(&mut map, hash, &[ptr])?;
            assert_collisions(&map, 0, 1)?;
            // Check internal state.
            prop_assert_eq!(collect_entries(&map), [entry(hash, ptr)]);
            prop_assert!(map.colliders.is_empty());
            prop_assert!(map.emptied_collider_slots.is_empty());

            // Test the colliders case.
            // First insert the rest of the `ptrs`.
            for ptr in &ptrs[1..] {
                prop_assert_eq!(map.insert(hash, *ptr), false);
            }
            assert_ptrs_and_len(&mut map, hash, &ptrs)?;
            assert_collisions(&map, ptrs.len(), 0)?;
            // Now try inserting `ptr` again.
            let old_map = map.clone(); // Savepoint
            prop_assert_eq!(map.insert(hash, ptr), true); // Insert again.
            prop_assert_eq!(&map, &old_map); // No change!
            // Check API state:
            assert_ptrs_and_len(&mut map, hash, &ptrs)?;
            assert_collisions(&map, ptrs.len(), 0)?;
            // Check internal state.
            prop_assert_eq!(collect_entries(&map), [(hash, ColliderSlotIndex::new(0).into())]);
            prop_assert_eq!(map.colliders, [ptrs.to_owned()]);
            prop_assert!(map.emptied_collider_slots.is_empty());
        }

        #[test]
        fn insert_same_ptr_under_diff_hash(
            (hashes, ptr) in (vec(any::<RowHash>(), 2..10), gen_row_pointer())
        ) {
            ensure_unique(&hashes)?;

            // Insert `ptr` under all `hashes`.
            let mut map = PointerMap::default();
            for hash in &hashes {
                prop_assert_eq!(map.insert(*hash, ptr), false);
            }
            // Check API state:
            for hash in &hashes {
                assert_ptrs_are(&mut map, *hash, &[ptr])?;
            }
            prop_assert_eq!(map.len(), hashes.len());
            prop_assert_eq!(map.is_empty(), false);
            assert_collisions(&map, 0, hashes.len())?;
            // Check internal state.
            let mut entries = collect_entries(&map);
            entries.sort();
            prop_assert_eq!(
                entries,
                hashes.iter().copied().sorted().map(|hash| entry(hash, ptr)).collect::<Vec<_>>()
            );
            prop_assert!(map.colliders.is_empty());
            prop_assert!(map.emptied_collider_slots.is_empty());
        }

        #[test]
        fn insert_different_for_same_hash_handles_collision(
            (hash, ptrs) in (any::<RowHash>(), vec(gen_row_pointer(), 3..10))
        ) {
            ensure_unique(&ptrs)?;

            let mut map = PointerMap::default();

            // Insert `0` -> no collision.
            prop_assert_eq!(map.insert(hash, ptrs[0]), false);
            // Check API state:
            assert_ptrs_and_len(&mut map, hash, &ptrs[..1])?;
            assert_collisions(&map, 0, 1)?;
            // Check internal state.
            prop_assert_eq!(collect_entries(&map), [entry(hash, ptrs[0])]);
            prop_assert!(map.colliders.is_empty());
            prop_assert!(map.emptied_collider_slots.is_empty());

            // Insert `1` => `0` and `1` collide.
            // This exercises "make new collider slot".
            prop_assert_eq!(map.insert(hash, ptrs[1]), false);
            // Check API state:
            assert_ptrs_and_len(&mut map, hash, &ptrs[..2])?;
            assert_collisions(&map, 2, 0)?;
            // Check internal state.
            let first_collider_idx = ColliderSlotIndex::new(0);
            let one_collider_entry = [(hash, first_collider_idx.into())];
            prop_assert_eq!(collect_entries(&map), one_collider_entry);
            prop_assert_eq!(&*map.colliders, [ptrs[..2].to_owned()]);
            prop_assert!(map.emptied_collider_slots.is_empty());

            // This exercises "reuse collider slot".
            for (ptr, i) in ptrs[2..].iter().copied().zip(2..) {
                // Insert `i = 2..`
                prop_assert_eq!(map.insert(hash, ptr), false);
                // Check API state:
                assert_ptrs_and_len(&mut map, hash, &ptrs[..=i])?;
                assert_collisions(&map, i + 1, 0)?;
                // Check internal state.
                prop_assert_eq!(collect_entries(&map), one_collider_entry);
                prop_assert_eq!(&*map.colliders, [ptrs[..=i].to_owned()]);
                prop_assert!(map.emptied_collider_slots.is_empty());
            }

            // Remove all but the last one.
            let last = ptrs.len() - 1;
            for ptr in &ptrs[..last] {
                prop_assert!(map.remove(hash, *ptr));
            }
            // Check API state:
            assert_ptrs_and_len(&mut map, hash, &ptrs[last..])?;
            assert_collisions(&map, 0, 1)?;
            // Check internal state.
            prop_assert_eq!(collect_entries(&map), [entry(hash, ptrs[last])]);
            prop_assert_eq!(&*map.colliders, [vec![]]);
            prop_assert_eq!(&*map.emptied_collider_slots, [first_collider_idx]);

            // Insert `pennultimate` => `last` and `pennultimate` collide.
            // This exercises "reuse collider slot".
            let penultimate = last - 1;
            prop_assert_eq!(map.insert(hash, ptrs[penultimate]), false);
            // Check API state:
            let pointers = ptrs[penultimate..].iter().copied().rev().collect::<Vec<_>>();
            assert_ptrs_and_len(&mut map, hash, &pointers)?;
            assert_collisions(&map, 2, 0)?;
            // Check internal state.
            prop_assert_eq!(collect_entries(&map), one_collider_entry);
            prop_assert_eq!(&*map.colliders, [pointers]);
            prop_assert!(map.emptied_collider_slots.is_empty());
        }

        #[test]
        fn remove_non_existing_fails((hash, ptr) in (any::<RowHash>(), gen_row_pointer())) {
            let mut map = PointerMap::default();
            prop_assert_eq!(map.remove(hash, ptr), false);
        }

        #[test]
        fn remove_uncollided_hash_works((hash, ptr) in (any::<RowHash>(), gen_row_pointer())) {
            let mut map = PointerMap::default();

            // Insert and then remove.
            prop_assert_eq!(map.insert(hash, ptr), false);
            prop_assert_eq!(map.remove(hash, ptr), true);
            // Check API state:
            assert_ptrs_and_len(&mut map, hash, &[])?;
            assert_collisions(&map, 0, 0)?;
            // Check internal state.
            prop_assert_eq!(collect_entries(&map), []);
            prop_assert!(map.colliders.is_empty());
            prop_assert!(map.emptied_collider_slots.is_empty());
        }

        #[test]
        fn remove_same_hash_wrong_ptr_fails(
            (hash, ptr_a, ptr_b) in (
                any::<RowHash>(),
                gen_row_pointer(),
                gen_row_pointer(),
            )
        ) {
            ensure_unique(&[ptr_a, ptr_b])?;

            let mut map = PointerMap::default();

            // Insert `ptr_a` and then remove `ptr_b`.
            prop_assert_eq!(map.insert(hash, ptr_a), false);
            prop_assert_eq!(map.remove(hash, ptr_b), false);
            // Check API state:
            assert_ptrs_and_len(&mut map, hash, &[ptr_a])?;
            assert_collisions(&map, 0, 1)?;
            // Check internal state.
            prop_assert_eq!(collect_entries(&map), [entry(hash, ptr_a)]);
            prop_assert!(map.colliders.is_empty());
            prop_assert!(map.emptied_collider_slots.is_empty());
        }

        #[test]
        fn remove_collided_hash_wrong_ptr_fails(
            (hash, ptrs) in (any::<RowHash>(), vec(gen_row_pointer(), 3..10))
        ) {
            ensure_unique(&ptrs)?;

            let mut map = PointerMap::default();

            // Insert `ptrs[0..last]` and then remove `ptrs[last]`.
            let last = ptrs.len() - 1;
            let but_last = &ptrs[0..last];
            for ptr in but_last {
                prop_assert_eq!(map.insert(hash, *ptr), false);
            }
            prop_assert_eq!(map.remove(hash, ptrs[last]), false);
            // Check API state:
            assert_ptrs_and_len(&mut map, hash, but_last)?;
            assert_collisions(&map, but_last.len(), 0)?;
            // Check internal state.
            prop_assert_eq!(collect_entries(&map), [(hash, ColliderSlotIndex::new(0).into())]);
            prop_assert_eq!(&*map.colliders, [but_last.to_owned()]);
            prop_assert!(map.emptied_collider_slots.is_empty());
        }

        #[test]
        fn remove_collided_hash_reduction_works(
            (hash, mut ptr_a, mut ptr_b, pick_b) in (
                any::<RowHash>(),
                gen_row_pointer(),
                gen_row_pointer(),
                any::<bool>(),
            )
        ) {
            ensure_unique(&[ptr_a, ptr_b])?;

            // Insert `ptr_a` and `ptr_b`.
            let mut map = PointerMap::default();
            prop_assert_eq!(map.insert(hash, ptr_a), false);
            prop_assert_eq!(map.insert(hash, ptr_b), false);
            assert_collisions(&map, 2, 0)?;

            // Now remove `ptr_a` or `ptr_b`.
            if pick_b {
                mem::swap(&mut ptr_a, &mut ptr_b);
            }
            prop_assert_eq!(map.remove(hash, ptr_b), true);
            assert_ptrs_and_len(&mut map, hash, &[ptr_a])?;
            assert_collisions(&map, 0, 1)?;
            prop_assert_eq!(map.emptied_collider_slots, [ColliderSlotIndex(0)]);
        }

        #[test]
        fn remove_collided_hash_works(
            (hash, mut ptrs, pick_remove_idx) in (
                any::<RowHash>(),
                vec(gen_row_pointer(), 3..10),
                0..10usize,
            )
        ) {
            ensure_unique(&ptrs)?;

            let pick_remove_idx = pick_remove_idx.min(ptrs.len() - 1);

            // Insert all in `ptrs`.
            let mut map = PointerMap::default();
            for ptr in &ptrs {
                prop_assert_eq!(map.insert(hash, *ptr), false);
            }
            assert_collisions(&map, ptrs.len(), 0)?;

            // Now remove `ptrs[pick_remove_idx]`.
            let ptr_to_remove = ptrs.remove(pick_remove_idx);
            prop_assert_eq!(map.remove(hash, ptr_to_remove), true);
            assert_ptrs_and_len(&mut map, hash, &ptrs)?;
            assert_collisions(&map, ptrs.len(), 0)?;
            prop_assert_eq!(sorted(&map.colliders[0]), sorted(&ptrs));
            prop_assert_eq!(map.emptied_collider_slots, []);
        }
    }
}
