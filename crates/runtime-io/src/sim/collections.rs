use core::{num::NonZeroUsize, ops::RangeBounds};

use alloc::collections::vec_deque::VecDeque;
use slab::Slab;

pub use slab::VacantEntry;

#[derive(Debug)]
pub struct BoundedVecDeque<T> {
    inner: VecDeque<T>,
}

impl<T> BoundedVecDeque<T> {
    pub fn with_capacity(n: NonZeroUsize) -> Self {
        Self {
            inner: VecDeque::with_capacity(n.get()),
        }
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.inner.pop_front()
    }

    pub fn swap_remove_front(&mut self, index: usize) -> Option<T> {
        self.inner.swap_remove_front(index)
    }

    pub fn push_back(&mut self, v: T) -> Result<(), T> {
        if self.inner.len() == self.inner.capacity() {
            Err(v)
        } else {
            self.inner.push_back(v);
            Ok(())
        }
    }

    pub fn extend<I>(&mut self, iter: I) -> Result<(), I::IntoIter>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let iter = iter.into_iter();
        if self.inner.len() + iter.len() > self.inner.capacity() {
            Err(iter)
        } else {
            self.inner.extend(iter);
            Ok(())
        }
    }

    pub fn drain(&mut self, range: impl RangeBounds<usize>) -> impl Iterator<Item = T> {
        self.inner.drain(range)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }

    pub fn clear(&mut self) {
        self.inner.clear()
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn remaining_capacity(&self) -> usize {
        self.inner.capacity() - self.inner.len()
    }
}

#[derive(Debug)]
pub struct BoundedSlab<T> {
    inner: Slab<T>,
}

impl<T> BoundedSlab<T> {
    pub fn with_capacity(n: NonZeroUsize) -> Self {
        Self {
            inner: Slab::with_capacity(n.get()),
        }
    }

    pub fn get(&self, key: usize) -> Option<&T> {
        self.inner.get(key)
    }

    pub fn get_mut(&mut self, key: usize) -> Option<&mut T> {
        self.inner.get_mut(key)
    }

    pub fn remove(&mut self, key: usize) -> T {
        self.inner.remove(key)
    }

    pub fn try_remove(&mut self, key: usize) -> Option<T> {
        self.inner.try_remove(key)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut T)> {
        self.inner.iter_mut()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.inner.drain()
    }

    pub fn vacant_entry(&mut self) -> Option<VacantEntry<'_, T>> {
        if self.inner.len() == self.inner.capacity() {
            None
        } else {
            Some(self.inner.vacant_entry())
        }
    }

    pub fn clear(&mut self) {
        self.inner.clear()
    }

    pub fn remaining_capacity(&self) -> usize {
        self.inner.capacity() - self.inner.len()
    }
}
