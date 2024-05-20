use core::{
    ops::{BitAnd, BitAndAssign, BitOr, Not, Shl},
    slice::Iter,
};
pub use internal_unsafe::FixedBitSet;
use internal_unsafe::Len;

/// A type used to represent blocks in a bit set.
/// A smaller type, compared to usize,
/// means taking less advantage of native operations.
/// A larger type means we might over-allocate more.
pub trait BitBlock:
    Copy
    + Eq
    + Not<Output = Self>
    + BitAnd<Self, Output = Self>
    + BitAndAssign
    + BitOr<Self, Output = Self>
    + Shl<usize, Output = Self>
{
    /// The number of bits that [`Self`] can represent.
    const BITS: u32;

    /// The first bit is set.
    const ONE: Self;

    /// No bits are set.
    const ZERO: Self;

    fn wrapping_sub(self, rhs: Self) -> Self;
    fn trailing_zeros(self) -> u32;
}

type DefaultBitBlock = u64;

impl BitBlock for DefaultBitBlock {
    const BITS: u32 = Self::BITS;
    const ONE: Self = 1;
    const ZERO: Self = 0;

    #[inline]
    fn wrapping_sub(self, rhs: Self) -> Self {
        self.wrapping_sub(rhs)
    }

    #[inline]
    fn trailing_zeros(self) -> u32 {
        self.trailing_zeros()
    }
}

/// The internals of `FixedBitSet`.
/// Separated from the higher level APIs to contain the safety boundary.
mod internal_unsafe {
    use super::{BitBlock, DefaultBitBlock};
    use crate::{static_assert_align, static_assert_size};
    use core::{
        mem,
        ptr::NonNull,
        slice::{from_raw_parts, from_raw_parts_mut},
    };

    /// Computes how many blocks are needed to store that many bits.
    fn blocks_for_bits<B: BitBlock>(bits: usize) -> usize {
        // Must round e.g., 31 / 32 to 1 and 32 / 32 to 1 as well.
        bits.div_ceil(B::BITS as usize)
    }

    /// The type used to represent the number of bits the set can hold.
    ///
    /// Currently `u16` to keep `mem::size_of::<FixedBitSet>()` small.
    pub(super) type Len = u16;

    /// A bit set that, once created, has a fixed size over time.
    ///
    /// The set can store at most `u16::MAX` number of bits.
    #[repr(C, packed)]
    pub struct FixedBitSet<B = DefaultBitBlock> {
        /// The size of the heap allocation in number of elements.
        len: Len,
        /// A pointer to a heap allocation of `[B]` of `self.len`.
        ptr: NonNull<B>,
    }

    static_assert_align!(FixedBitSet, 1);
    static_assert_size!(FixedBitSet, mem::size_of::<usize>() + mem::size_of::<Len>());

    // SAFETY: `FixedBitSet` owns its data.
    unsafe impl<B> Send for FixedBitSet<B> {}
    // SAFETY: `FixedBitSet` owns its data.
    unsafe impl<B> Sync for FixedBitSet<B> {}

    impl<B> Drop for FixedBitSet<B> {
        fn drop(&mut self) {
            let blocks = self.storage_mut();
            // SAFETY: We own the memory region pointed to by `blocks`,
            // and as we have `&'0 mut self`, we also have exclusive access to it.
            // So, and since we are in `drop`,
            // we can deallocate the memory as we are the last referent to it.
            // Moreover, the memory was allocated in `Self::new(..)` using `vec![..]`,
            // which will allocate using `Global`, so we can convert it back to a `Box`.
            let _ = unsafe { Box::from_raw(blocks) };
        }
    }

    impl<B: BitBlock> FixedBitSet<B> {
        /// Allocates a new bit set capable of holding `bits` number of bits.
        pub fn new(bits: usize) -> Self {
            // Compute the number of blocks needed.
            let nblocks = blocks_for_bits::<B>(bits);
            // SAFETY: required for the soundness of `Drop` as
            // `dealloc` must receive the same layout as it was `alloc`ated with.
            assert!(nblocks <= Len::MAX as usize);
            let len = nblocks as Len;

            // Allocate the blocks and extract the pointer to the heap region.
            let blocks: Box<[B]> = vec![B::ZERO; nblocks].into_boxed_slice();
            let ptr = NonNull::from(Box::leak(blocks)).cast();

            Self { ptr, len }
        }
    }

    impl<B> FixedBitSet<B> {
        /// Returns the backing `[B]` slice for shared access.
        pub(super) const fn storage(&self) -> &[B] {
            let ptr = self.ptr.as_ptr();
            let len = self.len as usize;
            // SAFETY:
            // - `self.ptr` is a `NonNull` so `ptr` cannot be null.
            // - `self.ptr` is properly aligned for `BitBlock`s.
            // - `self.ptr` is valid for reads as we have `&self` and we own the memory
            //   which we know is `blocks` elements long.
            // - As we have `&'0 self`, elsewhere cannot mutate the memory during `'0`
            //   except through an `UnsafeCell`.
            unsafe { from_raw_parts(ptr, len) }
        }

        /// Returns the backing `[B]` slice for mutation.
        pub(super) fn storage_mut(&mut self) -> &mut [B] {
            let ptr = self.ptr.as_ptr();
            let len = self.len as usize;
            // SAFETY:
            // - `self.ptr` is a `NonNull` so `ptr` cannot be null.
            // - `self.ptr` is properly aligned for `BitBlock`s.
            // - `self.ptr` is valid for reads and writes as we have `&mut self` and we own the memory
            //   which we know is `blocks` elements long.
            // - As we have `&'0 mut self`, we have exclusive access for `'0`
            //   so the memory cannot be accessed elsewhere during `'0`.
            unsafe { from_raw_parts_mut(ptr, len) }
        }
    }
}

impl<B: BitBlock> FixedBitSet<B> {
    /// Converts `idx` to its block index and the index within the block.
    const fn idx_to_pos(idx: usize) -> (usize, usize) {
        let bits = B::BITS as usize;
        (idx / bits, idx % bits)
    }

    /// Returns whether `idx` is set or not.
    pub fn get(&self, idx: usize) -> bool {
        let (block_idx, pos_in_block) = Self::idx_to_pos(idx);
        let block = self.storage()[block_idx];
        (block & (B::ONE << pos_in_block)) != B::ZERO
    }

    /// Sets bit at position `idx` to `val`.
    pub fn set(&mut self, idx: usize, val: bool) {
        let (block_idx, pos_in_block) = Self::idx_to_pos(idx);
        let block = &mut self.storage_mut()[block_idx];

        // Update the block.
        let flag = B::ONE << pos_in_block;
        *block = if val { *block | flag } else { *block & !flag };
    }

    /// Clears every bit in the vec.
    pub fn clear(&mut self) {
        self.storage_mut().fill(B::ZERO);
    }

    /// Returns all the set indices.
    pub fn iter_set(&self) -> IterSet<'_, B> {
        let mut inner = self.storage().iter();

        // Fetch the first block; if it isn't there, use an all-zero one.
        // This will cause the iterator to terminate immediately.
        let curr = inner.next().copied().unwrap_or(B::ZERO);

        IterSet {
            inner,
            curr,
            block_idx: 0,
        }
    }

    /// Returns all the set indices from `start_idx` inclusive.
    pub fn iter_set_from(&self, start_idx: usize) -> IterSet<'_, B> {
        // Translate the index to its block and position within it.
        let (block_idx, pos_in_block) = Self::idx_to_pos(start_idx);

        // We want our iteration to start from the block that includes `start_idx`.
        let mut inner = self.storage()[block_idx..].iter();

        // Fetch the first block; if it isn't there, use an all-zero one.
        // This will cause the iterator to terminate immediately.
        let curr = inner.next().copied().unwrap_or(B::ZERO);

        // Our `start_idx` might be in the middle of the `curr` block.
        // To resolve this, we must zero out any preceding bits.
        // So e.g., for `B = u8`,
        // we must transform `0000_1011` to `0000_1000` for `start_idx = 3`.
        let zero_preceding_mask = B::ZERO.wrapping_sub(B::ONE << pos_in_block);
        let curr = curr & zero_preceding_mask;

        IterSet {
            inner,
            curr,
            block_idx: block_idx as Len,
        }
    }
}

/// An iterator that yields the set indices of a [`FixedBitSet`].
pub struct IterSet<'a, B = DefaultBitBlock> {
    /// The block iterator.
    inner: Iter<'a, B>,
    /// The current block being processed, taken from `self.inner`.
    curr: B,
    /// What the index of `self.curr` is.
    block_idx: Len,
}

impl<B: BitBlock> Iterator for IterSet<'_, B> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let tz = self.curr.trailing_zeros();
            if tz < B::BITS {
                // Some bit was set; so yield the index of that
                // and zero the bit out so we don't yield it again.
                self.curr &= self.curr.wrapping_sub(B::ONE);
                let idx = self.block_idx as u32 * B::BITS + tz;
                return Some(idx as usize);
            } else {
                // No bit is set; advance to the next block, or quit if none left.
                self.curr = *self.inner.next()?;
                self.block_idx += 1;
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use proptest::bits::bitset::between;
    use proptest::prelude::*;
    use spacetimedb_data_structures::map::HashSet;

    #[test]
    #[should_panic]
    fn zero_sized_is_ok() {
        let mut set = FixedBitSet::<DefaultBitBlock>::new(0);
        set.clear();
        set.iter_set_from(0).count();
        set.iter_set().count();
        set.get(0);
    }

    const MAX_NBITS: usize = 1000;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(if cfg!(miri) { 8 } else { 2048 }))]

        #[test]
        fn after_new_there_are_no_bits_set(nbits in 0..MAX_NBITS) {
            let set = FixedBitSet::<DefaultBitBlock>::new(nbits);
            for idx in 0..nbits {
                prop_assert!(!set.get(idx));
            }
        }

        #[test]
        fn after_clear_there_are_no_bits_set(choices in between(0, MAX_NBITS)) {
            let nbits = choices.get_ref().len();

            let mut set = FixedBitSet::<DefaultBitBlock>::new(nbits);

            // Set all the bits chosen.
            for idx in &choices {
                prop_assert!(!set.get(idx));
                set.set(idx, true);
                prop_assert!(set.get(idx));
            }

            // Clear!
            set.clear();

            // After clearing, all bits should be unset.
            for idx in 0..nbits {
                prop_assert!(!set.get(idx));
            }
        }

        #[test]
        fn get_set_consistency(choices in between(0, MAX_NBITS)) {
            let nbits = choices.get_ref().len();
            let mut set = FixedBitSet::<DefaultBitBlock>::new(nbits);

            // Set all the bits chosen.
            for idx in &choices {
                prop_assert!(!set.get(idx));

                // After setting, it's true.
                set.set(idx, true);
                prop_assert!(set.get(idx));
                // And this is idempotent.
                set.set(idx, true);
                prop_assert!(set.get(idx));
            }

            // Build the "complement" of `choices`.
            let choices: HashSet<_> = choices.into_iter().collect();
            let universe: HashSet<_> = (0..nbits).collect();
            for idx in universe.difference(&choices) {
                prop_assert!(!set.get(*idx));
            }

            // Unset all the bits chosen.
            for idx in &choices {
                // After unsetting, it's false.
                set.set(*idx, false);
                prop_assert!(!set.get(*idx));
                // And this is idempotent.
                set.set(*idx, false);
                prop_assert!(!set.get(*idx));
            }
        }

        #[test]
        fn iter_set_preserves_order_of_original_choices(choices in between(0, MAX_NBITS)) {
            let nbits = choices.get_ref().len();

            // Set all the bits chosen.
            let mut set = FixedBitSet::<DefaultBitBlock>::new(nbits);
            for idx in &choices {
                set.set(idx, true);
            }

            // `iter_set` produces the same list `choices`.
            let collected = set.iter_set().collect::<Vec<_>>();
            let original = choices.iter().collect::<Vec<_>>();
            prop_assert_eq!(&original, &collected);

            if let [_, second, ..] = &*original {
                // Starting from the second yields the same list as `choices[1..]`.
                let collected = set.iter_set_from(*second).collect::<Vec<_>>();
                prop_assert_eq!(&original[1..], &collected);
            }

            // `iter_set_from` and `iter_set` produce the same list.
            prop_assert_eq!(collected, set.iter_set_from(0).collect::<Vec<_>>());
        }

    }
}
