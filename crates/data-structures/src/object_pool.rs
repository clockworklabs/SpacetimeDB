use core::any::type_name;
use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};
use crossbeam_queue::ArrayQueue;
use std::sync::Arc;

#[cfg(not(feature = "memory-usage"))]
/// An object that can be put into a [`Pool<T>`].
pub trait PooledObject {}

#[cfg(feature = "memory-usage")]
/// An object that can be put into a [`Pool<T>`].
///
/// The trait exposes hooks that the pool needs
/// so that it can e.g., implement `MemoryUsage`.
pub trait PooledObject: spacetimedb_memory_usage::MemoryUsage {
    /// The storage for the number of bytes in the pool.
    ///
    /// When each object in the pool takes up the same size, this can be `()`.
    /// Otherwise, it will typically be [`AtomicUsize`].
    type ResidentBytesStorage: Default;

    /// Returns the number of bytes resident in the pool.
    ///
    /// The `storage` is provided as well as the `num_objects` in the pool.
    /// Typically, exactly one of these will be used.
    fn resident_object_bytes(storage: &Self::ResidentBytesStorage, num_objects: usize) -> usize;

    /// Called by the pool to add `bytes` to `storage`, if necessary.
    fn add_to_resident_object_bytes(storage: &Self::ResidentBytesStorage, bytes: usize);

    /// Called by the pool to subtract `bytes` from `storage`, if necessary.
    fn sub_from_resident_object_bytes(storage: &Self::ResidentBytesStorage, bytes: usize);
}

/// A pool of some objects of type `T`.
pub struct Pool<T: PooledObject> {
    inner: Arc<Inner<T>>,
}

impl<T: PooledObject> fmt::Debug for Pool<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dropped = self.dropped_count();
        let new = self.new_allocated_count();
        let reused = self.reused_count();
        let returned = self.returned_count();

        #[cfg(feature = "memory-usage")]
        let bytes = T::resident_object_bytes(&self.inner.resident_object_bytes, self.inner.objects.len());

        let mut builder = f.debug_struct(&format!("Pool<{}>", type_name::<T>()));

        #[cfg(feature = "memory-usage")]
        let builder = builder.field("resident_object_bytes", &bytes);

        builder
            .field("dropped_count", &dropped)
            .field("new_allocated_count", &new)
            .field("reused_count", &reused)
            .field("returned_count", &returned)
            .finish()
    }
}

impl<T: PooledObject> Clone for Pool<T> {
    fn clone(&self) -> Self {
        let inner = self.inner.clone();
        Self { inner }
    }
}

#[cfg(feature = "memory-usage")]
impl<T: PooledObject> spacetimedb_memory_usage::MemoryUsage for Pool<T> {
    fn heap_usage(&self) -> usize {
        let Self { inner } = self;
        inner.heap_usage()
    }
}

impl<T: PooledObject> Pool<T> {
    /// Returns a new pool with a maximum capacity of `cap`.
    /// This capacity is fixed over the lifetime of the pool.
    pub fn new(cap: usize) -> Self {
        let inner = Arc::new(Inner::new(cap));
        Self { inner }
    }

    /// Puts back an object into the pool.
    pub fn put(&self, object: T) {
        self.inner.put(object);
    }

    /// Puts back an object into the pool.
    pub fn put_many(&self, objects: impl Iterator<Item = T>) {
        for obj in objects {
            self.put(obj);
        }
    }

    /// Takes an object from the pool or creates a new one.
    pub fn take(&self, clear: impl FnOnce(&mut T), new: impl FnOnce() -> T) -> T {
        self.inner.take(clear, new)
    }

    /// Returns the number of pages dropped by the pool because the pool was at capacity.
    pub fn dropped_count(&self) -> usize {
        self.inner.dropped_count.load(Ordering::Relaxed)
    }

    /// Returns the number of fresh objects allocated through the pool.
    pub fn new_allocated_count(&self) -> usize {
        self.inner.new_allocated_count.load(Ordering::Relaxed)
    }

    /// Returns the number of objects reused from the pool.
    pub fn reused_count(&self) -> usize {
        self.inner.reused_count.load(Ordering::Relaxed)
    }

    /// Returns the number of objects returned to the pool.
    pub fn returned_count(&self) -> usize {
        self.inner.returned_count.load(Ordering::Relaxed)
    }
}

/// The inner actual page pool containing all the logic.
struct Inner<T: PooledObject> {
    objects: ArrayQueue<T>,
    dropped_count: AtomicUsize,
    new_allocated_count: AtomicUsize,
    reused_count: AtomicUsize,
    returned_count: AtomicUsize,

    #[cfg(feature = "memory-usage")]
    resident_object_bytes: T::ResidentBytesStorage,
}

#[cfg(feature = "memory-usage")]
impl<T: PooledObject> spacetimedb_memory_usage::MemoryUsage for Inner<T> {
    fn heap_usage(&self) -> usize {
        let Self {
            objects,
            dropped_count,
            new_allocated_count,
            reused_count,
            returned_count,
            resident_object_bytes,
        } = self;
        dropped_count.heap_usage() +
        new_allocated_count.heap_usage() +
        reused_count.heap_usage() +
        returned_count.heap_usage() +
        // This is the amount the queue itself takes up on the heap.
        objects.capacity() * size_of::<(AtomicUsize, T)>() +
        // This is the amount the objects take up on the heap, excluding the static size.
        T::resident_object_bytes(resident_object_bytes, objects.len())
    }
}

#[inline]
fn inc(atomic: &AtomicUsize) {
    atomic.fetch_add(1, Ordering::Relaxed);
}

impl<T: PooledObject> Inner<T> {
    /// Creates a new pool capable of holding `cap` objects.
    fn new(cap: usize) -> Self {
        let objects = ArrayQueue::new(cap);
        Self {
            objects,
            dropped_count: <_>::default(),
            new_allocated_count: <_>::default(),
            reused_count: <_>::default(),
            returned_count: <_>::default(),

            #[cfg(feature = "memory-usage")]
            resident_object_bytes: <_>::default(),
        }
    }

    /// Puts back an object into the pool.
    fn put(&self, object: T) {
        #[cfg(feature = "memory-usage")]
        let bytes = object.heap_usage();
        // Add it to the pool if there's room, or just drop it.
        if self.objects.push(object).is_ok() {
            #[cfg(feature = "memory-usage")]
            T::add_to_resident_object_bytes(&self.resident_object_bytes, bytes);

            inc(&self.returned_count);
        } else {
            inc(&self.dropped_count);
        }
    }

    /// Takes an object from the pool or creates a new one.
    ///
    /// The closure `clear` provides the opportunity to clear the object before use.
    /// The closure `new` is called to create a new object when the pool is empty.
    fn take(&self, clear: impl FnOnce(&mut T), new: impl FnOnce() -> T) -> T {
        self.objects
            .pop()
            .map(|mut object| {
                #[cfg(feature = "memory-usage")]
                T::sub_from_resident_object_bytes(&self.resident_object_bytes, object.heap_usage());

                inc(&self.reused_count);
                clear(&mut object);
                object
            })
            .unwrap_or_else(|| {
                inc(&self.new_allocated_count);
                new()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{iter, ptr::addr_eq};

    // The type of pools used for testing.
    // We want to include a `Box` so that we can do pointer comparisons.
    type P = Pool<Box<i32>>;

    #[cfg(not(feature = "memory-usage"))]
    impl PooledObject for Box<i32> {}

    #[cfg(feature = "memory-usage")]
    impl PooledObject for Box<i32> {
        type ResidentBytesStorage = ();
        fn add_to_resident_object_bytes(_: &Self::ResidentBytesStorage, _: usize) {}
        fn sub_from_resident_object_bytes(_: &Self::ResidentBytesStorage, _: usize) {}
        fn resident_object_bytes(_: &Self::ResidentBytesStorage, num_objects: usize) -> usize {
            num_objects * size_of::<i32>()
        }
    }

    fn new() -> P {
        P::new(100)
    }

    fn assert_metrics(pool: &P, dropped: usize, new: usize, reused: usize, returned: usize) {
        assert_eq!(pool.dropped_count(), dropped);
        assert_eq!(pool.new_allocated_count(), new);
        assert_eq!(pool.reused_count(), reused);
        assert_eq!(pool.returned_count(), returned);
    }

    fn take(pool: &P) -> Box<i32> {
        pool.take(|_| {}, || Box::new(0))
    }

    #[test]
    fn pool_returns_same_obj() {
        let pool = new();
        assert_metrics(&pool, 0, 0, 0, 0);

        // Create an object and put it back.
        let obj1 = take(&pool);
        assert_metrics(&pool, 0, 1, 0, 0);
        let obj1_ptr = &*obj1 as *const _;
        pool.put(obj1);
        assert_metrics(&pool, 0, 1, 0, 1);

        // Extract an object again.
        let obj2 = take(&pool);
        assert_metrics(&pool, 0, 1, 1, 1);
        let obj2_ptr = &*obj2 as *const _;
        // It should be the same as the previous one.
        assert!(addr_eq(obj1_ptr, obj2_ptr));
        pool.put(obj2);
        assert_metrics(&pool, 0, 1, 1, 2);

        // Extract an object again.
        let obj3 = take(&pool);
        assert_metrics(&pool, 0, 1, 2, 2);
        let obj3_ptr = &*obj3 as *const _;
        // It should be the same as the previous one.
        assert!(addr_eq(obj1_ptr, obj3_ptr));

        // Manually create an object and put it in.
        let obj4 = Box::new(0);
        let obj4_ptr = &*obj4 as *const _;
        pool.put(obj4);
        pool.put(obj3);
        assert_metrics(&pool, 0, 1, 2, 4);
        // When we take out an object, it should be the same as `obj4` and not `obj1`.
        let obj5 = take(&pool);
        assert_metrics(&pool, 0, 1, 3, 4);
        let obj5_ptr = &*obj5 as *const _;
        // Same as obj4.
        assert!(!addr_eq(obj5_ptr, obj1_ptr));
        assert!(addr_eq(obj5_ptr, obj4_ptr));
    }

    #[test]
    fn pool_drops_past_max_size() {
        const N: usize = 3;
        let pool = P::new(N);

        let pages = iter::repeat_with(|| take(&pool)).take(N + 1).collect::<Vec<_>>();
        assert_metrics(&pool, 0, N + 1, 0, 0);

        pool.put_many(pages.into_iter());
        assert_metrics(&pool, 1, N + 1, 0, N);
        assert_eq!(pool.inner.objects.len(), N);
    }
}
