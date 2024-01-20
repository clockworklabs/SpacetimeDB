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
    ops::Deref,
    ptr::NonNull,
    slice::from_raw_parts,
};
use either::Either;

/// Constructs a `ColList` like so `col_list![0, 2]`.
///
/// A head element is required.
/// Mostly provided for testing.
#[macro_export]
macro_rules! col_list {
    ($head:expr $(, $elem:expr)* $(,)?) => {{
        let mut list = $crate::ColList::new($head.into());
        $(list.push($elem.into());)*
        list
    }};
}

/// An error signalling that a `ColList` was empty.
#[derive(Debug)]
pub struct EmptyColListError;

/// A builder for a [`ColList`] making sure a non-empty one is built.
pub struct ColListBuilder {
    /// The in-progress list.
    list: ColList,
}

impl Default for ColListBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ColListBuilder {
    /// Returns an empty builder.
    pub fn new() -> Self {
        let list = ColList::from_inline(0);
        Self { list }
    }

    /// Returns an empty builder with a capacity for a list of `cap` elements.
    pub fn with_capacity(cap: u32) -> Self {
        let list = ColList::with_capacity(cap);
        Self { list }
    }

    /// Push a [`ColId`] to the list.
    pub fn push(&mut self, col: ColId) {
        self.list.push(col);
    }

    /// Build the [`ColList`] or error if it would have been empty.
    pub fn build(self) -> Result<ColList, EmptyColListError> {
        if self.list.is_empty() {
            Err(EmptyColListError)
        } else {
            Ok(self.list)
        }
    }
}

impl FromIterator<ColId> for ColListBuilder {
    fn from_iter<T: IntoIterator<Item = ColId>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let (lower_bound, _) = iter.size_hint();
        let mut builder = Self::with_capacity(lower_bound as u32);
        for col in iter {
            builder.push(col);
        }
        builder
    }
}

/// This represents a non-empty list of [`ColId`]
/// but packed into a `u64` in a way that takes advantage of the fact that
/// in almost all cases, we won't store a `ColId` larger than 62.
/// In the rare case that we store larger ids, we fall back to a thin vec approach.
///
/// The list does not guarantee a stable order.
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

impl From<u32> for ColList {
    fn from(value: u32) -> Self {
        ColId(value).into()
    }
}

impl From<ColId> for ColList {
    fn from(value: ColId) -> Self {
        Self::new(value)
    }
}

impl ColList {
    /// Returns a list with a single column.
    /// As long `col` is below `62`, this will not allocate.
    pub fn new(col: ColId) -> Self {
        let mut list = Self::from_inline(0);
        list.push(col);
        list
    }

    /// Returns an empty list with a capacity to hold `cap` elements.
    fn with_capacity(cap: u32) -> Self {
        // We speculate that all elements < `Self::FIRST_HEAP_COL`.
        if cap < Self::FIRST_HEAP_COL as u32 {
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

    /// Returns the head of the list.
    pub fn head(&self) -> ColId {
        // SAFETY: There's always at least one element in the list when this is called.
        // Notably, `from_inline(0)` is followed by at least one `.push(col)` before
        // a safe `ColList` is exposed outside this module.
        unsafe { self.iter().next().unwrap_unchecked() }
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
    pub fn iter(&self) -> impl '_ + Iterator<Item = ColId> {
        match self.as_inline() {
            Ok(inline) => Either::Left(inline.iter()),
            Err(heap) => Either::Right(heap.iter().copied()),
        }
    }

    /// Convert to a `Vec<u32>`.
    pub fn to_u32_vec(&self) -> alloc::vec::Vec<u32> {
        self.iter().map(u32::from).collect()
    }

    /// Returns the length of the list.
    pub fn len(&self) -> u32 {
        match self.as_inline() {
            Ok(inline) => inline.len(),
            Err(heap) => heap.len(),
        }
    }

    /// Returns false. A `ColList` is never empty.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Push `col` onto the list.
    ///
    /// If `col >= 63` or if this list was already heap allocated, it will now be heap allocated.
    pub fn push(&mut self, col: ColId) {
        let val = u32::from(col) as u64;
        match (val < Self::FIRST_HEAP_COL, self.as_inline_mut()) {
            (true, Ok(inline)) => inline.0 |= 1 << (val + 1),
            // Converts the list to its non-inline heap form.
            // This is unlikely to happen.
            (false, Ok(inline)) => *self = Self::from_heap(inline.heapify_and_push(col)),
            (_, Err(heap)) => heap.push(col),
        }
    }

    /// The first `ColId` that would make the list heap allocated.
    const FIRST_HEAP_COL: u64 = size_of::<u64>() as u64 * 8 - 1;

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
        // This bit is unused by the heap case as the pointer must be aligned for `u32`.
        // SAFETY: Even when `inline`, and on a < 64-bit target,
        // we can treat the union as a `usize` to check the lowest bit.
        let addr = unsafe { self.check };
        addr & 1 != 0
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

/// The inline version of a [`ColList`].
#[derive(Clone, Copy, PartialEq)]
struct ColListInline(u64);

impl ColListInline {
    /// Returns whether `needle` is part of this list.
    fn contains(&self, needle: ColId) -> bool {
        let col = needle.0;
        let inline = self.undo_mark();
        col < ColList::FIRST_HEAP_COL as u32 && inline & (1u64 << col) != 0
    }

    /// Returns an iterator over the [`ColId`]s stored by this list.
    fn iter(&self) -> impl '_ + Iterator<Item = ColId> {
        let mut value = self.undo_mark();
        iter::from_fn(move || {
            if value == 0 {
                // No set bits; quit!
                None
            } else {
                // Count trailing zeros and then zero out the first set bit.
                // For e.g., `0b11001`, this would yield `[0, 3, 4]` as expected.
                let id = ColId(value.trailing_zeros());
                value &= value.wrapping_sub(1);
                Some(id)
            }
        })
    }

    /// Returns the length of the list.
    fn len(&self) -> u32 {
        self.undo_mark().count_ones()
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
struct ColListVec(NonNull<u32>);

impl ColListVec {
    /// Returns an empty vector with `capacity`.
    fn with_capacity(capacity: u32) -> Self {
        // Allocate the vector using the global allocator.
        let layout = Self::layout(capacity);
        // SAFETY: the size of `[u32; 2 + capacity]` is always non-zero.
        let ptr = unsafe { alloc(layout) }.cast::<u32>();
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
    fn len(&self) -> u32 {
        let ptr = self.0.as_ptr();
        // SAFETY: `ptr` is properly aligned for `u32` and is valid for reads.
        unsafe { *ptr }
    }

    /// SAFETY: `new_len <= self.capacity()` and `new_len` <= number of initialized elements.
    unsafe fn set_len(&mut self, new_len: u32) {
        let ptr = self.0.as_ptr();
        // SAFETY:
        // - `ptr` is valid for writes as we have exclusive access.
        // - It's also properly aligned for `u32`.
        unsafe {
            *ptr = new_len;
        }
    }

    /// Returns the capacity of the allocation in terms of elements.
    fn capacity(&self) -> u32 {
        let ptr = self.0.as_ptr();
        // SAFETY: `ptr + 1 u32` is in bounds of the allocation and it doesn't overflow isize.
        let capacity_ptr = unsafe { ptr.add(1) };
        // SAFETY: `capacity_ptr` is properly aligned for `u32` and is valid for reads.
        unsafe { *capacity_ptr }
    }

    /// Sets the capacity of the allocation in terms of elements.
    ///
    /// SAFETY: `cap` must match the actual capacity of the allocation.
    unsafe fn set_capacity(&mut self, cap: u32) {
        let ptr = self.0.as_ptr();
        // SAFETY: `ptr + 1 u32` is in bounds of the allocation and it doesn't overflow isize.
        let cap_ptr = unsafe { ptr.add(1) };
        // SAFETY: `cap_ptr` is valid for writes as we have ownership of the allocation.
        // It's also properly aligned for `u32`.
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
        // SAFETY: Allocated for `2 + capacity` `u32`s and `len <= capacity`, so we're in bounds.
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
    ///     len: u32,
    ///     capacity: u32,
    ///     data: [ColId],
    /// }
    /// ```
    ///
    /// Panics if `cap` would result in an allocation larger than `isize::MAX`.
    fn layout(cap: u32) -> Layout {
        Layout::array::<u32>(cap.checked_add(2).expect("capacity overflow") as usize).unwrap()
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::collection::vec;
    use proptest::prelude::*;

    fn contains(list: &ColList, x: &ColId) -> bool {
        list.iter().any(|y| y == *x)
    }

    proptest! {
        #[test]
        fn test_inline(cols in vec((0..ColList::FIRST_HEAP_COL as u32).prop_map_into(), 1..100)) {
            let [head, tail @ ..] = &*cols else { unreachable!() };

            let mut list = ColList::new(*head);
            prop_assert!(list.is_inline());
            prop_assert!(!list.is_empty());
            prop_assert_eq!(list.len(), 1);
            prop_assert_eq!(list.head(), *head);
            prop_assert_eq!(list.iter().collect::<Vec<_>>(), [*head]);

            for col in tail {
                let new_head = list.head().min(*col);
                list.push(*col);

                prop_assert!(list.is_inline());
                prop_assert!(!list.is_empty());
                prop_assert_eq!(list.head(), new_head);
                prop_assert!(contains(&list, col));
            }

            prop_assert_eq!(&list.clone(), &list);

            let mut cols = cols;
            cols.sort();
            cols.dedup();
            prop_assert_eq!(list.iter().collect::<Vec<_>>(), cols);
        }

        #[test]
        fn test_heap(cols in vec((ColList::FIRST_HEAP_COL as u32.. ).prop_map_into(), 1..100)) {
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
                prop_assert_eq!(list.head(), head);


                prop_assert!(contains(&list, col));
            }

            prop_assert_eq!(&list.clone(), &list);

            let mut cols = cols;
            cols.insert(0, head);
            prop_assert_eq!(list.iter().collect::<Vec<_>>(), cols);
        }
    }
}
