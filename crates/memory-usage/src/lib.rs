use core::mem;
use core::sync::atomic::AtomicUsize;

/// For inspecting how much memory a value is using.
///
/// This trait specifically measures heap memory. If you want to measure stack memory too, add
/// `mem::size_of_val()` to it. (This only really matters for the outermost type in a hierarchy.)
pub trait MemoryUsage {
    /// The **heap** memory usage of this type. The default implementation returns 0.
    #[inline(always)]
    fn heap_usage(&self) -> usize {
        0
    }
}

impl MemoryUsage for () {}
impl MemoryUsage for bool {}
impl MemoryUsage for u8 {}
impl MemoryUsage for u16 {}
impl MemoryUsage for u32 {}
impl MemoryUsage for u64 {}
impl MemoryUsage for u128 {}
#[cfg(feature = "ethnum")]
impl MemoryUsage for ethnum::u256 {}
impl MemoryUsage for usize {}
impl MemoryUsage for AtomicUsize {}
impl MemoryUsage for i8 {}
impl MemoryUsage for i16 {}
impl MemoryUsage for i32 {}
impl MemoryUsage for i64 {}
impl MemoryUsage for i128 {}
#[cfg(feature = "ethnum")]
impl MemoryUsage for ethnum::i256 {}
impl MemoryUsage for isize {}
impl MemoryUsage for f32 {}
impl MemoryUsage for f64 {}
#[cfg(feature = "decorum")]
impl MemoryUsage for decorum::Total<f32> {}
#[cfg(feature = "decorum")]
impl MemoryUsage for decorum::Total<f64> {}

impl<T: MemoryUsage + ?Sized> MemoryUsage for &T {
    fn heap_usage(&self) -> usize {
        (*self).heap_usage()
    }
}

impl<T: MemoryUsage + ?Sized> MemoryUsage for Box<T> {
    fn heap_usage(&self) -> usize {
        mem::size_of_val::<T>(self) + T::heap_usage(self)
    }
}

impl<T: MemoryUsage + ?Sized> MemoryUsage for std::sync::Arc<T> {
    fn heap_usage(&self) -> usize {
        let refcounts = mem::size_of::<usize>() * 2;
        refcounts + mem::size_of_val::<T>(self) + T::heap_usage(self)
    }
}

impl<T: MemoryUsage + ?Sized> MemoryUsage for std::rc::Rc<T> {
    fn heap_usage(&self) -> usize {
        let refcounts = mem::size_of::<usize>() * 2;
        refcounts + mem::size_of_val::<T>(self) + T::heap_usage(self)
    }
}

impl<T: MemoryUsage> MemoryUsage for [T] {
    fn heap_usage(&self) -> usize {
        self.iter().map(T::heap_usage).sum()
    }
}

impl<T: MemoryUsage, const N: usize> MemoryUsage for [T; N] {
    fn heap_usage(&self) -> usize {
        self.iter().map(T::heap_usage).sum()
    }
}

impl MemoryUsage for str {}

impl<T: MemoryUsage> MemoryUsage for Option<T> {
    fn heap_usage(&self) -> usize {
        self.as_ref().map_or(0, T::heap_usage)
    }
}

impl<A: MemoryUsage, B: MemoryUsage> MemoryUsage for (A, B) {
    fn heap_usage(&self) -> usize {
        self.0.heap_usage() + self.1.heap_usage()
    }
}

impl MemoryUsage for String {
    fn heap_usage(&self) -> usize {
        self.capacity()
    }
}

impl<T: MemoryUsage> MemoryUsage for Vec<T> {
    fn heap_usage(&self) -> usize {
        self.capacity() * mem::size_of::<T>() + self.iter().map(T::heap_usage).sum::<usize>()
    }
}

#[cfg(feature = "hashbrown")]
impl<K: MemoryUsage + Eq + core::hash::Hash, V: MemoryUsage, S: core::hash::BuildHasher> MemoryUsage
    for hashbrown::HashMap<K, V, S>
{
    fn heap_usage(&self) -> usize {
        self.allocation_size() + self.iter().map(|(k, v)| k.heap_usage() + v.heap_usage()).sum::<usize>()
    }
}

impl<K: MemoryUsage, V: MemoryUsage> MemoryUsage for std::collections::BTreeMap<K, V> {
    fn heap_usage(&self) -> usize {
        // NB: this is best-effort, since we don't have a `capacity()` method on `BTreeMap`.
        self.len() * mem::size_of::<(K, V)>() + self.iter().map(|(k, v)| k.heap_usage() + v.heap_usage()).sum::<usize>()
    }
}

#[cfg(feature = "smallvec")]
impl<A: smallvec::Array> MemoryUsage for smallvec::SmallVec<A>
where
    A::Item: MemoryUsage,
{
    fn heap_usage(&self) -> usize {
        self.as_slice().heap_usage()
            + if self.spilled() {
                self.capacity() * mem::size_of::<A::Item>()
            } else {
                0
            }
    }
}
