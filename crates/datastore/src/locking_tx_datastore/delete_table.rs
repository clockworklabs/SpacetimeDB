use spacetimedb_table::{
    fixed_bit_set::FixedBitSet,
    indexes::{max_rows_in_page, PageIndex, PageOffset, RowPointer, Size, SquashedOffset},
};

/// A table recording which rows of a table in the [`CommittedState`] that have been deleted.
pub struct DeleteTable {
    /// Keeps track of all the deleted row pointers.
    deleted: Vec<Option<FixedBitSet>>,
    /// The number of deleted row pointers.
    ///
    /// This is stored for efficiency, but can be derived from `self.deleted` otherwise.
    len: usize,
    /// The size of a row in the table.
    fixed_row_size: Size,
}

impl DeleteTable {
    /// Returns a new deletion table where the rows have `fixed_row_size`.
    ///
    /// The table is initially empty.
    pub fn new(fixed_row_size: Size) -> Self {
        Self {
            deleted: <_>::default(),
            len: 0,
            fixed_row_size,
        }
    }

    /// Returns whether `ptr`, belonging to a table in [`CommittedState`], is recorded as deleted.
    pub fn contains(&self, ptr: RowPointer) -> bool {
        let page_idx = ptr.page_index().idx();
        match self.deleted.get(page_idx) {
            Some(Some(set)) => set.get(ptr.page_offset() / self.fixed_row_size),
            _ => false,
        }
    }

    /// Marks `ptr`, belonging to a table in [`CommittedState`], as deleted.
    ///
    /// Returns `true` if `ptr` was not previously marked.
    pub fn insert(&mut self, ptr: RowPointer) -> bool {
        let fixed_row_size = self.fixed_row_size;
        let page_idx = ptr.page_index().idx();
        let bitset_idx = ptr.page_offset() / fixed_row_size;

        let new_set = || {
            let mut bs = FixedBitSet::new(max_rows_in_page(fixed_row_size));
            bs.set(bitset_idx, true);
            bs
        };

        match self.deleted.get_mut(page_idx) {
            // Already got a bitset for this page, just set the bit.
            Some(Some(set)) => {
                let added = !set.get(bitset_idx);
                set.set(bitset_idx, true);
                if added {
                    self.len += 1;
                }
                added
            }
            // No bitset yet, initialize the slot with a new one.
            Some(slot) => {
                *slot = Some(new_set());
                self.len += 1;
                true
            }
            // We haven't reached this page index before,
            // Make uninitialized slots for all the pages before this one
            // that do not have bitsets.
            // Add an initialized bitset for this page index.
            None => {
                let pages = self.deleted.len();
                let after = 1 + page_idx;
                self.deleted.reserve(after - pages);
                for _ in pages..page_idx {
                    self.deleted.push(None);
                }
                self.deleted.push(Some(new_set()));
                self.len += 1;
                true
            }
        }
    }

    /// Un-marks `ptr`, belonging to a table in [`CommittedState`], as deleted.
    pub fn remove(&mut self, ptr: RowPointer) -> bool {
        let fixed_row_size = self.fixed_row_size;
        let page_idx = ptr.page_index().idx();
        let bitset_idx = ptr.page_offset() / fixed_row_size;
        if let Some(Some(set)) = self.deleted.get_mut(page_idx) {
            let removed = set.get(bitset_idx);
            if removed {
                self.len -= 1;
            }
            set.set(bitset_idx, false);
            removed
        } else {
            false
        }
    }

    /// Yields all the row pointers marked in this table.
    pub fn iter(&self) -> impl '_ + Iterator<Item = RowPointer> {
        (0..)
            .map(PageIndex)
            .zip(self.deleted.iter())
            .filter_map(|(pi, set)| Some((pi, set.as_ref()?)))
            .flat_map(move |(pi, set)| {
                set.iter_set().map(move |idx| {
                    let po = PageOffset(idx as u16 * self.fixed_row_size.0);
                    // It's a committed state pointer that has been deleted.
                    RowPointer::new(false, pi, po, SquashedOffset::COMMITTED_STATE)
                })
            })
    }

    /// Returns the number of rows marked for deletion.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether there are any rows to delete.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use core::cmp::Ordering;
    use proptest::array::uniform;
    use proptest::collection::vec;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    struct OrdRowPtr(RowPointer);
    impl Ord for OrdRowPtr {
        fn cmp(&self, other: &Self) -> Ordering {
            self.0
                .page_index()
                .cmp(&other.0.page_index())
                .then_with(|| self.0.page_offset().cmp(&other.0.page_offset()))
        }
    }
    impl PartialOrd for OrdRowPtr {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
    impl PartialEq<OrdRowPtr> for RowPointer {
        fn eq(&self, other: &OrdRowPtr) -> bool {
            *self == other.0
        }
    }

    /// The `DeleteTable` is really just a set specialized for `RowPointer`.
    /// To harden the tests, we mirror the operations with a `BTreeSet`
    /// and assert equivalent observable results.
    ///
    /// The setup here can cause some redundancy in our checks,
    /// but that's fine, as testing is not perf sensitive.
    struct TestDT {
        dt: DeleteTable,
        bs: BTreeSet<OrdRowPtr>,
    }

    impl TestDT {
        fn new(fixed_row_size: Size) -> Self {
            let dt = DeleteTable::new(fixed_row_size);
            let bs = BTreeSet::new();
            Self { dt, bs }
        }
        fn contains(&self, ptr: RowPointer) -> bool {
            let dt = self.dt.contains(ptr);
            let bs = self.bs.contains(&OrdRowPtr(ptr));
            assert_eq!(dt, bs);
            dt
        }
        fn insert(&mut self, ptr: RowPointer) -> bool {
            let dt = self.dt.insert(ptr);
            let bs = self.bs.insert(OrdRowPtr(ptr));
            assert_eq!(dt, bs);
            self.check_state();
            dt
        }
        fn remove(&mut self, ptr: RowPointer) -> bool {
            let dt = self.dt.remove(ptr);
            let bs = self.bs.remove(&OrdRowPtr(ptr));
            assert_eq!(dt, bs);
            self.check_state();
            dt
        }
        fn iter(&self) -> impl Iterator<Item = RowPointer> {
            let dt = self.dt.iter().collect::<Vec<_>>();
            let bs = self.bs.iter().copied().collect::<Vec<_>>();
            assert_eq!(dt, bs);
            assert_eq!(self.len(), bs.len());
            dt.into_iter()
        }
        fn check_state(&self) {
            let _ = self.iter();
        }
        fn len(&self) -> usize {
            let dt = self.dt.len();
            let bs = self.bs.len();
            assert_eq!(dt, bs);
            dt
        }
        fn is_empty(&self) -> bool {
            let dt = self.dt.is_empty();
            let bs = self.bs.is_empty();
            assert_eq!(dt, bs);
            dt
        }
    }

    fn gen_size() -> impl Strategy<Value = Size> {
        (2..100u16).prop_map(Size)
    }

    fn gen_ptr(row_size: Size) -> impl Strategy<Value = RowPointer> {
        let page_offset = (0..100u16).prop_map(move |num| PageOffset(row_size.0 * num));
        let page_index = (0..100u64).prop_map(PageIndex);
        (page_index, page_offset).prop_map(|(pi, po)| RowPointer::new(false, pi, po, SquashedOffset::COMMITTED_STATE))
    }

    fn gen_size_and_ptrs<const N: usize>() -> impl Strategy<Value = (Size, [RowPointer; N])> {
        gen_size().prop_flat_map(|s| uniform(gen_ptr(s)).prop_map(move |ptr| (s, ptr)))
    }

    fn gen_two_ptr_vecs() -> impl Strategy<Value = (Size, [Vec<RowPointer>; 2])> {
        gen_size().prop_flat_map(|s| uniform(vec(gen_ptr(s), 0..100)).prop_map(move |vs| (s, vs)))
    }

    proptest! {
        #[test]
        fn insertion_entails_contained((size, [ptr_a, ptr_b]) in gen_size_and_ptrs()) {
            prop_assume!(ptr_a != ptr_b);

            let mut dt = TestDT::new(size);

            // Initially we have nothing.
            prop_assert!(dt.is_empty());
            prop_assert!(!dt.contains(ptr_a));
            prop_assert!(!dt.contains(ptr_b));

            // Add `ptr_a` and expect it but not `ptr_b`.
            prop_assert!(dt.insert(ptr_a));
            prop_assert!(!dt.is_empty());
            prop_assert!(dt.contains(ptr_a));
            prop_assert!(!dt.contains(ptr_b));
        }

        #[test]
        fn insertion_is_state_idempotent((size, [ptr_a]) in gen_size_and_ptrs()) {
            let mut dt = TestDT::new(size);

            prop_assert!(dt.insert(ptr_a));
            prop_assert!(dt.contains(ptr_a));

            prop_assert!(!dt.insert(ptr_a)); // Idempotence.
            prop_assert!(dt.contains(ptr_a));
        }

        #[test]
        fn deleting_non_existent_does_nothing((size, [ptr_a, ptr_b]) in gen_size_and_ptrs()) {
            prop_assume!(ptr_a != ptr_b);

            let mut dt = TestDT::new(size);

            prop_assert!(!dt.remove(ptr_b));
            prop_assert!(dt.insert(ptr_a));
            prop_assert!(!dt.remove(ptr_b));
        }

        #[test]
        fn insertion_followed_by_deletion_is_no_op((size, [ptr_a]) in gen_size_and_ptrs()) {
            let mut dt = TestDT::new(size);

            prop_assert!(dt.insert(ptr_a));
            prop_assert!(dt.contains(ptr_a));

            prop_assert!(dt.remove(ptr_a));
            prop_assert!(!dt.contains(ptr_a));
        }

        #[test]
        fn set_intersection_behaves((size, [ptrs_a, ptrs_b]) in gen_two_ptr_vecs()) {
            let mut dt_a = TestDT::new(size);
            let mut dt_b = TestDT::new(size);

            // Fill first set.
            for ptr in ptrs_a {
                dt_a.insert(ptr);
            }
            // Fill second set.
            for ptr in ptrs_b {
                dt_b.insert(ptr);
            }

            // Intersect first and second sets.
            let expected_intersection = dt_a.bs.intersection(&dt_b.bs).copied().collect::<BTreeSet<_>>();
            for ptr in dt_a.iter() {
                if !dt_b.contains(ptr) {
                    prop_assert!(dt_a.remove(ptr));
                }
            }
            prop_assert_eq!(dt_a.iter().collect::<Vec<_>>(), expected_intersection.into_iter().collect::<Vec<_>>());
        }
    }
}
