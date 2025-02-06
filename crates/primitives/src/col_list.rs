#![forbid(unsafe_op_in_unsafe_fn)]

extern crate alloc;

use crate::ColId;
use alloc::alloc::{alloc, dealloc, handle_alloc_error, realloc};
use core::{
    alloc::Layout,
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    iter,
    mem::{size_of, ManuallyDrop},
    ops::{Deref, DerefMut},
    ptr::NonNull,
    slice::{from_raw_parts, from_raw_parts_mut},
};
use either::Either;
use itertools::Itertools;

/// Constructs a `ColList` like so `col_list![0, 2]`.
///
/// Mostly provided for testing.
#[macro_export]
macro_rules! col_list {
    ($($elem:expr),* $(,)?) => {{
        $crate::ColList::from([$($elem),*])
    }};
}

/// This represents a list of [`ColId`]s
/// but packed into a `u64` in a way that takes advantage of the fact that
/// in almost all cases, we won't store a `ColId` larger than 62.
/// In the rare case that we store larger ids, we fall back to a thin vec approach.
///
/// We also fall back to a thin vec if the ids stored are not in sorted order, from low to high,
/// or if the list contains duplicates.
///
/// If you want a set of columns, use [`ColSet`] instead. It is more likely to be compressed,
/// and so is a better choice if you don't require ordering information.
#[repr(C)]
pub union ColList {
    /// Used to determine whether the list is stored inline or not.
    check: usize,
    /// The list is stored inline as a bitset.
    inline: ColListInline,
    /// A heap allocated version of the list.
    heap: ManuallyDrop<ColListVec>,
}

// SAFETY: The data is owned by `ColList` so this is OK.
unsafe impl Sync for ColList {}
// SAFETY: The data is owned by `ColList` so this is OK.
unsafe impl Send for ColList {}

impl<C: Into<ColId>> From<C> for ColList {
    fn from(value: C) -> Self {
        Self::new(value.into())
    }
}

impl<C: Into<ColId>, const N: usize> From<[C; N]> for ColList {
    fn from(cols: [C; N]) -> Self {
        cols.map(|c| c.into()).into_iter().collect()
    }
}

impl<C: Into<ColId>> FromIterator<C> for ColList {
    fn from_iter<I: IntoIterator<Item = C>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let (lower_bound, _) = iter.size_hint();
        let mut list = Self::with_capacity(lower_bound as u16);
        list.extend(iter);
        list
    }
}

impl<C: Into<ColId>> Extend<C> for ColList {
    fn extend<T: IntoIterator<Item = C>>(&mut self, iter: T) {
        let iter = iter.into_iter();
        for col in iter {
            self.push(col.into());
        }
    }
}

impl Default for ColList {
    fn default() -> Self {
        Self::with_capacity(0)
    }
}

impl ColList {
    /// Returns an empty list.
    pub fn empty() -> Self {
        Self::from_inline(0)
    }

    /// Returns a list with a single column.
    /// As long `col` is below `62`, this will not allocate.
    pub fn new(col: ColId) -> Self {
        let mut list = Self::from_inline(0);
        list.push_inner(col, true);
        list
    }

    /// Returns an empty list with a capacity to hold `cap` elements.
    pub fn with_capacity(cap: u16) -> Self {
        // We speculate that all elements < `Self::FIRST_HEAP_COL`.
        if cap < Self::FIRST_HEAP_COL_U16 {
            Self::from_inline(0)
        } else {
            Self::from_heap(ColListVec::with_capacity(cap))
        }
    }

    /// Constructs a list from a `u64` bitset
    /// where the highest bit is unset.
    ///
    /// Panics in debug mode if the highest bit is set.
    fn from_inline(list: u64) -> Self {
        debug_assert_eq!(list & (1 << Self::FIRST_HEAP_COL), 0);
        // (1) Move the whole inline bitset by one bit to the left.
        // Mark the now-zero lowest bit so we know the list is inline.
        let inline = ColListInline(list << 1 | 1);
        // SAFETY: Lowest bit is set, so this will be interpreted as inline and not a pointer.
        let ret = Self { inline };
        debug_assert!(ret.is_inline());
        ret
    }

    /// Constructs a list in heap form from `vec`.
    fn from_heap(vec: ColListVec) -> Self {
        let heap = ManuallyDrop::new(vec);
        Self { heap }
    }

    /// Returns `head` if that is the only element.
    pub fn as_singleton(&self) -> Option<ColId> {
        let mut iter = self.iter();
        match (iter.next(), iter.next()) {
            (h @ Some(_), None) => h,
            _ => None,
        }
    }

    /// Returns the head of the list, if any.
    pub fn head(&self) -> Option<ColId> {
        self.iter().next()
    }

    /// Returns the last of the list, if any.
    pub fn last(&self) -> Option<ColId> {
        match self.as_inline() {
            Ok(inline) => inline.last(),
            Err(heap) => heap.last().copied(),
        }
    }

    /// Returns whether `needle` is contained in the list.
    ///
    /// This an be faster than using `list.iter().any(|c| c == needle)`.
    pub fn contains(&self, needle: ColId) -> bool {
        match self.as_inline() {
            Ok(inline) => inline.contains(needle),
            Err(heap) => heap.contains(&needle),
        }
    }

    /// Returns an iterator over all the columns in this list.
    pub fn iter(&self) -> impl '_ + Clone + Iterator<Item = ColId> {
        match self.as_inline() {
            Ok(inline) => Either::Left(inline.iter()),
            Err(heap) => Either::Right(heap.iter().copied()),
        }
    }

    /// Convert to a `Box<[u16]>`.
    pub fn to_u16_vec(&self) -> alloc::boxed::Box<[u16]> {
        self.iter().map(u16::from).collect()
    }

    /// Returns the length of the list.
    pub fn len(&self) -> u16 {
        match self.as_inline() {
            Ok(inline) => inline.len(),
            Err(heap) => heap.len(),
        }
    }

    /// Returns whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Push `col` onto the list.
    ///
    /// If `col >= 63` or `col <= last_col`, the list will become heap allocated if not already.
    pub fn push(&mut self, col: ColId) {
        self.push_inner(col, self.last().map_or(true, |l| l < col));
    }

    /// Sort and deduplicate the list.
    /// If the list is already sorted and deduplicated, does nothing.
    /// This will typically result in an inline list unless there are large `ColId`s in play.
    fn sort_dedup(&mut self) {
        if let Err(heap) = self.as_inline_mut() {
            heap.sort();

            // Don't reallocate if the list is already sorted and deduplicated.
            let is_deduped = is_sorted_and_deduped(heap);
            let wants_inline = heap.last().unwrap_or(&ColId(0)).0 < Self::FIRST_HEAP_COL_U16;
            if !is_deduped || wants_inline {
                *self = Self::from_iter(heap.iter().copied().dedup());
            }
        }
    }

    /// Push `col` onto the list.
    ///
    /// If `col >= 63` or `!preserves_set_order`,
    /// the list will become heap allocated if not already.
    #[inline]
    fn push_inner(&mut self, col: ColId, preserves_set_order: bool) {
        let val = u16::from(col) as u64;
        match (val < Self::FIRST_HEAP_COL && preserves_set_order, self.as_inline_mut()) {
            (true, Ok(inline)) => inline.0 |= 1 << (val + 1),
            // Converts the list to its non-inline heap form.
            // This is unlikely to happen.
            (false, Ok(inline)) => *self = Self::from_heap(inline.heapify_and_push(col)),
            (_, Err(heap)) => heap.push(col),
        }
    }

    /// The first `ColId` that would make the list heap allocated.
    const FIRST_HEAP_COL: u64 = size_of::<u64>() as u64 * 8 - 1;

    /// The first `ColId` that would make the list heap allocated.
    const FIRST_HEAP_COL_U16: u16 = Self::FIRST_HEAP_COL as u16;

    /// Returns the list either as inline or heap based.
    #[inline]
    fn as_inline(&self) -> Result<&ColListInline, &ManuallyDrop<ColListVec>> {
        if self.is_inline() {
            // SAFETY: confirmed that it is inline so this field is active.
            Ok(unsafe { &self.inline })
        } else {
            // SAFETY: confirmed it's not, so `heap` is active instead.
            Err(unsafe { &self.heap })
        }
    }

    /// Returns the list either as inline or heap based.
    #[inline]
    fn as_inline_mut(&mut self) -> Result<&mut ColListInline, &mut ManuallyDrop<ColListVec>> {
        if self.is_inline() {
            // SAFETY: confirmed that it is inline so this field is active.
            Ok(unsafe { &mut self.inline })
        } else {
            // SAFETY: confirmed it's not, so `heap` is active instead.
            Err(unsafe { &mut self.heap })
        }
    }

    #[inline]
    fn is_inline(&self) -> bool {
        // Check whether the lowest bit has been marked.
        // This bit is unused by the heap case as the pointer must be aligned for `u16`.
        // That is, we know that if the `self.heap` variant is active,
        // then `self.heap.addr() % align_of::<u16> == 0`.
        // So if `self.check % align_of::<u16> == 1`, as checked below,
        // we now it's the inline case and not the heap case.

        // SAFETY: Even when `inline`, and on a < 64-bit target,
        // we can treat the union as a `usize` to check the lowest bit.
        let addr = unsafe { self.check };
        addr & 1 != 0
    }

    #[doc(hidden)]
    pub fn heap_size(&self) -> usize {
        match self.as_inline() {
            Ok(_) => 0,
            Err(heap) => heap.capacity() as usize,
        }
    }
}

impl Drop for ColList {
    fn drop(&mut self) {
        if let Err(heap) = self.as_inline_mut() {
            // SAFETY: Only called once, so we will not have use-after-free or double-free.
            unsafe { ManuallyDrop::drop(heap) };
        }
    }
}

impl Clone for ColList {
    fn clone(&self) -> Self {
        match self.as_inline() {
            Ok(inline) => Self { inline: *inline },
            Err(heap) => Self { heap: heap.clone() },
        }
    }
}

impl Eq for ColList {}
impl PartialEq for ColList {
    fn eq(&self, other: &Self) -> bool {
        match (self.as_inline(), other.as_inline()) {
            (Ok(lhs), Ok(rhs)) => lhs == rhs,
            (Err(lhs), Err(rhs)) => ***lhs == ***rhs,
            _ => false,
        }
    }
}

impl Ord for ColList {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}
impl PartialOrd for ColList {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for ColList {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.as_inline() {
            Ok(inline) => inline.0.hash(state),
            Err(heap) => heap.hash(state),
        }
    }
}

impl fmt::Debug for ColList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl From<ColSet> for ColList {
    fn from(value: ColSet) -> Self {
        value.0
    }
}

/// A borrowed list of columns or a single one.
pub enum ColOrCols<'a> {
    /// A single column.
    Col(ColId),
    /// A list of columns.
    ColList(&'a ColList),
}

impl ColOrCols<'_> {
    /// Returns `Some(col)` iff `self` is singleton.
    pub fn as_singleton(&self) -> Option<ColId> {
        match self {
            Self::Col(col) => Some(*col),
            Self::ColList(cols) => cols.as_singleton(),
        }
    }

    /// Returns an iterator over all the columns in this list.
    pub fn iter(&self) -> impl '_ + Iterator<Item = ColId> {
        match self {
            Self::Col(col) => Either::Left(iter::once(*col)),
            Self::ColList(cols) => Either::Right(cols.iter()),
        }
    }

    /// Returns the length of this list.
    pub fn len(&self) -> u16 {
        match self {
            Self::Col(_) => 1,
            Self::ColList(cols) => cols.len(),
        }
    }

    /// Returns whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Converts to a [`ColList`].
    pub fn to_owned(self) -> ColList {
        match self {
            Self::Col(col) => [col].into(),
            Self::ColList(list) => list.clone(),
        }
    }
}

impl PartialEq<ColList> for ColOrCols<'_> {
    fn eq(&self, other: &ColList) -> bool {
        self.iter().eq(other.iter())
    }
}
impl PartialEq for ColOrCols<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for ColOrCols<'_> {}
impl Ord for ColOrCols<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}
impl PartialOrd for ColOrCols<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A compressed set of columns. Like a `ColList`, but guaranteed to be sorted and to contain no duplicate entries.
/// Dereferences to a `ColList` for convenience.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColSet(ColList);

impl ColSet {
    /// Check if a `ColSet` contains a given column.
    pub fn contains(&self, needle: ColId) -> bool {
        match self.as_inline() {
            Ok(inline) => inline.contains(needle),
            // We can use binary search because the vector is guaranteed to be sorted.
            Err(heap) => heap.binary_search(&needle).is_ok(),
        }
    }

    // Don't implement `insert` because repeated insertions will be O(n^2) if we want to keep the set sorted on the heap.
    // Use iterator methods to create a new `ColSet` instead.
}

impl<C: Into<ColId>> FromIterator<C> for ColSet {
    fn from_iter<T: IntoIterator<Item = C>>(iter: T) -> Self {
        // TODO: implement a fast path here that avoids allocation, by lying about
        // `preserves_set_order` to `push_inner`.
        Self::from(iter.into_iter().collect::<ColList>())
    }
}

impl From<ColList> for ColSet {
    fn from(mut list: ColList) -> Self {
        list.sort_dedup();
        Self(list)
    }
}

impl From<&ColList> for ColSet {
    fn from(value: &ColList) -> Self {
        value.iter().collect()
    }
}

impl From<ColOrCols<'_>> for ColSet {
    fn from(value: ColOrCols<'_>) -> Self {
        match value {
            ColOrCols::Col(col) => ColSet(col.into()),
            ColOrCols::ColList(cols) => cols.into(),
        }
    }
}

impl<C: Into<ColId>> From<C> for ColSet {
    fn from(value: C) -> Self {
        Self::from(ColList::new(value.into()))
    }
}

impl Deref for ColSet {
    type Target = ColList;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for ColSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

/// The inline version of a [`ColList`].
#[derive(Clone, Copy, PartialEq)]
struct ColListInline(u64);

impl ColListInline {
    /// Returns whether `needle` is part of this list.
    fn contains(&self, needle: ColId) -> bool {
        let col = needle.0;
        let inline = self.undo_mark();
        col < ColList::FIRST_HEAP_COL_U16 && inline & (1u64 << col) != 0
    }

    /// Returns an iterator over the [`ColId`]s stored by this list.
    fn iter(&self) -> impl '_ + Clone + Iterator<Item = ColId> {
        let mut value = self.undo_mark();
        iter::from_fn(move || {
            if value == 0 {
                // No set bits; quit!
                None
            } else {
                // Count trailing zeros and then zero out the first set bit.
                // For e.g., `0b11001`, this would yield `[0, 3, 4]` as expected.
                let id = ColId(value.trailing_zeros() as u16);
                value &= value.wrapping_sub(1);
                Some(id)
            }
        })
    }

    /// Returns the last element of the list.
    fn last(&self) -> Option<ColId> {
        (u64::BITS - self.undo_mark().leading_zeros())
            .checked_sub(1)
            .map(|c| ColId(c as _))
    }

    /// Returns the length of the list.
    fn len(&self) -> u16 {
        self.undo_mark().count_ones() as u16
    }

    /// Undoes the shift in (1).
    #[inline]
    fn undo_mark(&self) -> u64 {
        self.0 >> 1
    }

    /// Returns an equivalent list in heap form instead of inline, and adds `col` to it.
    /// The capacity of the vec will be `2 * (self.len() + 1)`
    fn heapify_and_push(&self, col: ColId) -> ColListVec {
        let mut vec = ColListVec::with_capacity(2 * (self.len() + 1));
        for col in self.iter() {
            vec.push(col)
        }
        vec.push(col);
        vec
    }
}

/// The thin-vec heap based version of a [`ColList`].
struct ColListVec(NonNull<u16>);

impl ColListVec {
    /// Returns an empty vector with `capacity`.
    fn with_capacity(capacity: u16) -> Self {
        // Allocate the vector using the global allocator.
        let layout = Self::layout(capacity);
        // SAFETY: the size of `[u16; 2 + capacity]` is always non-zero.
        let ptr = unsafe { alloc(layout) }.cast::<u16>();
        let Some(ptr_non_null) = NonNull::new(ptr) else {
            handle_alloc_error(layout)
        };

        let mut this = Self(ptr_non_null);
        // SAFETY: `0 <= capacity` and claiming no elements are init trivially holds.
        unsafe {
            this.set_len(0);
        }
        // SAFETY: `capacity` matches that of the allocation.
        unsafe { this.set_capacity(capacity) };
        this
    }

    /// Returns the length of the list.
    fn len(&self) -> u16 {
        let ptr = self.0.as_ptr();
        // SAFETY: `ptr` is properly aligned for `u16` and is valid for reads.
        unsafe { *ptr }
    }

    /// SAFETY: `new_len <= self.capacity()` and `new_len` <= number of initialized elements.
    unsafe fn set_len(&mut self, new_len: u16) {
        let ptr = self.0.as_ptr();
        // SAFETY:
        // - `ptr` is valid for writes as we have exclusive access.
        // - It's also properly aligned for `u16`.
        unsafe {
            *ptr = new_len;
        }
    }

    /// Returns the capacity of the allocation in terms of elements.
    fn capacity(&self) -> u16 {
        let ptr = self.0.as_ptr();
        // SAFETY: `ptr + 1 u16` is in bounds of the allocation and it doesn't overflow isize.
        let capacity_ptr = unsafe { ptr.add(1) };
        // SAFETY: `capacity_ptr` is properly aligned for `u16` and is valid for reads.
        unsafe { *capacity_ptr }
    }

    /// Sets the capacity of the allocation in terms of elements.
    ///
    /// SAFETY: `cap` must match the actual capacity of the allocation.
    unsafe fn set_capacity(&mut self, cap: u16) {
        let ptr = self.0.as_ptr();
        // SAFETY: `ptr + 1 u16` is in bounds of the allocation and it doesn't overflow isize.
        let cap_ptr = unsafe { ptr.add(1) };
        // SAFETY: `cap_ptr` is valid for writes as we have ownership of the allocation.
        // It's also properly aligned for `u16`.
        unsafe {
            *cap_ptr = cap;
        }
    }

    /// Push an element to the list.
    fn push(&mut self, val: ColId) {
        let len = self.len();
        let cap = self.capacity();

        if len == cap {
            // We're at capacity, reallocate using standard * 2 exponential factor.
            let new_cap = cap.checked_mul(2).expect("capacity overflow");
            let new_layout = Self::layout(new_cap);
            // Reallocation will will move the data as well.
            let old_layout = Self::layout(cap);
            let old_ptr = self.0.as_ptr().cast();
            // SAFETY:
            // - `base_ptr` came from the global allocator
            // - `old_layout` is the same layout used for the original allocation.
            // - `new_layout.size()` is non-zero and <= `isize::MAX`.
            let new_ptr = unsafe { realloc(old_ptr, old_layout, new_layout.size()) }.cast();
            let Some(ptr_non_null) = NonNull::new(new_ptr) else {
                handle_alloc_error(new_layout);
            };
            // Use new pointer and set capacity.
            self.0 = ptr_non_null;
            // SAFETY: `new_cap` matches that of the allocation.
            unsafe { self.set_capacity(new_cap) };
        }

        // Write the element and increase the length.
        let base_ptr = self.0.as_ptr();
        let elem_offset = 2 + len as usize;
        // SAFETY: Allocated for `2 + capacity` `u16`s and `len <= capacity`, so we're in bounds.
        let elem_ptr = unsafe { base_ptr.add(elem_offset) }.cast();
        // SAFETY: `elem_ptr` is valid for writes and is properly aligned for `ColId`.
        unsafe {
            *elem_ptr = val;
        }
        // SAFETY: the length <= the capacity and we just init the `len + 1`th element.
        unsafe {
            self.set_len(len + 1);
        }
    }

    /// Computes a layout for the following struct:
    /// ```rust,ignore
    /// struct ColListVecData {
    ///     len: u16,
    ///     capacity: u16,
    ///     data: [ColId],
    /// }
    /// ```
    ///
    /// Panics if `cap` would result in an allocation larger than `isize::MAX`.
    fn layout(cap: u16) -> Layout {
        Layout::array::<u16>(cap.checked_add(2).expect("capacity overflow") as usize).unwrap()
    }
}

impl Deref for ColListVec {
    type Target = [ColId];

    fn deref(&self) -> &Self::Target {
        let len = self.len() as usize;
        let ptr = self.0.as_ptr();
        // SAFETY: `ptr + 2` is always in bounds of the allocation and `ptr <= isize::MAX`.
        let ptr = unsafe { ptr.add(2) }.cast::<ColId>();
        // SAFETY:
        // - `ptr` is valid for reads for `len * size_of::<ColId>` and it is properly aligned.
        // - `len`  elements are initialized.
        // - For the lifetime of `'0`, the memory won't be mutated.
        // - `len * size_of::<ColId> <= isize::MAX` holds.
        unsafe { from_raw_parts(ptr, len) }
    }
}

impl DerefMut for ColListVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let len = self.len() as usize;
        let ptr = self.0.as_ptr();
        // SAFETY: `ptr + 2` is always in bounds of the allocation and `ptr <= isize::MAX`.
        let ptr = unsafe { ptr.add(2) }.cast::<ColId>();
        // SAFETY:
        // - `ptr` is valid for reads and writes for `len * size_of::<ColId>` and it is properly aligned.
        // - `len`  elements are initialized.
        // - `len * size_of::<ColId> <= isize::MAX` holds.
        unsafe { from_raw_parts_mut(ptr, len) }
    }
}

impl Drop for ColListVec {
    fn drop(&mut self) {
        let capacity = self.capacity();
        let layout = Self::layout(capacity);
        let ptr = self.0.as_ptr().cast();
        // SAFETY: `ptr` was allocated by the global allocator
        // and `layout` was the one the memory was allocated with.
        unsafe { dealloc(ptr, layout) };
    }
}

impl Clone for ColListVec {
    fn clone(&self) -> Self {
        let mut vec = ColListVec::with_capacity(self.len());
        for col in self.iter().copied() {
            vec.push(col);
        }
        vec
    }
}

/// Check if a buffer is sorted and deduplicated.
fn is_sorted_and_deduped(data: &[ColId]) -> bool {
    match data {
        [] => true,
        [mut prev, rest @ ..] => !rest.iter().any(|elem| {
            let bad = prev >= *elem;
            prev = *elem;
            bad
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::collection::vec;
    use proptest::prelude::*;

    fn contains(list: &ColList, x: &ColId) -> bool {
        list.iter().any(|y| y == *x)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(if cfg!(miri) { 8 } else { 2048 }))]

        #[test]
        fn test_inline(cols in vec((0..ColList::FIRST_HEAP_COL_U16).prop_map_into(), 1..100)) {
            let [head, tail @ ..] = &*cols else { unreachable!() };

            let mut list = ColList::new(*head);
            let mut is_inline = list.is_inline();
            prop_assert!(is_inline);
            prop_assert!(!list.is_empty());
            prop_assert_eq!(list.len(), 1);
            prop_assert_eq!(list.head(), Some(*head));
            prop_assert_eq!(list.last(), Some(*head));
            prop_assert_eq!(list.iter().collect::<Vec<_>>(), [*head]);


            for col in tail {
                is_inline &= list.last().unwrap() < *col;
                list.push(*col);

                prop_assert_eq!(is_inline, list.is_inline());
                prop_assert!(!list.is_empty());
                prop_assert_eq!(list.head(), Some(*head));
                prop_assert_eq!(list.last(), Some(*col));
                prop_assert_eq!(list.last(), list.iter().last());
                prop_assert!(contains(&list, col));
            }

            prop_assert_eq!(&list.clone(), &list);
            prop_assert_eq!(list.iter().collect::<Vec<_>>(), cols);
        }

        #[test]
        fn test_heap(cols in vec((ColList::FIRST_HEAP_COL_U16..).prop_map_into(), 1..100)) {
            let contains = |list: &ColList, x| list.iter().collect::<Vec<_>>().contains(x);

            let head = ColId(0);
            let mut list = ColList::new(head);
            prop_assert!(list.is_inline());
            prop_assert_eq!(list.len(), 1);

            for (idx, col) in cols.iter().enumerate() {
                list.push(*col);
                prop_assert!(!list.is_inline());
                prop_assert!(!list.is_empty());
                prop_assert_eq!(list.len() as usize, idx + 2);
                prop_assert_eq!(list.head(), Some(head));
                prop_assert_eq!(list.last(), Some(*col));
                prop_assert!(contains(&list, col));
            }

            prop_assert_eq!(&list.clone(), &list);

            let mut cols = cols;
            cols.insert(0, head);
            prop_assert_eq!(list.iter().collect::<Vec<_>>(), cols);
        }

        #[test]
        fn test_collect(cols in vec((0..100).prop_map_into(), 0..100)) {
            let list = cols.iter().copied().collect::<ColList>();
            prop_assert!(list.iter().eq(cols));
            prop_assert_eq!(&list, &list.iter().collect::<ColList>());
        }

        #[test]
        fn test_as_singleton(cols in vec((0..100).prop_map_into(), 0..10)) {
            let list = cols.iter().copied().collect::<ColList>();
            match cols.len() {
                1 => {
                    prop_assert_eq!(list.as_singleton(), Some(cols[0]));
                    prop_assert_eq!(list.as_singleton(), list.head());
                },
                _ => prop_assert_eq!(list.as_singleton(), None),
            }
        }

        #[test]
        fn test_set_inlines(mut cols in vec((0..ColList::FIRST_HEAP_COL_U16).prop_map_into(), 1..100)) {
            prop_assume!(!is_sorted_and_deduped(&cols[..]));

            let list = ColList::from_iter(cols.iter().copied());
            prop_assert!(!list.is_inline());
            let set = ColSet::from(list);
            prop_assert!(set.is_inline());

            for col in cols.iter() {
                prop_assert!(set.contains(*col));
            }

            cols.sort();
            cols.dedup();
            prop_assert_eq!(set.iter().collect::<Vec<_>>(), cols);
        }

        #[test]
        fn test_set_heap(mut cols in vec((ColList::FIRST_HEAP_COL_U16..).prop_map_into(), 1..100)) {
            prop_assume!(!is_sorted_and_deduped(&cols[..]));

            let list = ColList::from_iter(cols.iter().copied());
            prop_assert!(!list.is_inline());
            let set = ColSet::from(list);
            prop_assert!(!set.is_inline());

            for col in cols.iter() {
                prop_assert!(set.contains(*col));
            }

            cols.sort();
            cols.dedup();
            prop_assert_eq!(set.iter().collect::<Vec<_>>(), cols);
        }
    }
}
