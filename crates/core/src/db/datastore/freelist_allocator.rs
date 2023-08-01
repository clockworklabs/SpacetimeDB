use std::cell::RefCell;

use parking_lot::Mutex;

pub use inner::Handle;

pub struct Allocator<T> {
    inner: Mutex<inner::Inner<T>>,
}

impl<T> Allocator<T> {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(inner::Inner::new()),
        }
    }

    pub fn alloc(&self, t: T) -> Handle {
        let mut guard = self.inner.lock();
        guard.alloc(t)
    }

    pub fn free(&self, handle: Handle) -> T {
        let mut guard = self.inner.lock();
        guard.free(handle)
    }

    pub fn get(&self, handle: &Handle) -> &RefCell<T> {
        let ptr = {
            let mut guard = self.inner.lock();
            guard.get(handle)
        };
        unsafe { ptr.as_ref() }
    }
}

mod inner {
    use std::cell::{Cell, RefCell};

    type StableCell<T> = Box<Option<RefCell<T>>>;

    type NotSyncMarker = std::marker::PhantomData<Cell<()>>;

    #[derive(Debug)]
    pub struct Handle {
        idx: usize,
        _not_sync: NotSyncMarker,
    }

    impl Handle {
        fn new(idx: usize) -> Self {
            Self {
                idx,
                _not_sync: NotSyncMarker {},
            }
        }
    }

    // This is a freelist of StableCells. We maintain the following invariants:
    //
    // 0. Handles are not Clone and not Sync.
    // 1. Inner::slots is append-only.
    //      - Therefore, StableCells are never deallocated.
    // 2. StableCells are either:
    //      - None, in which case there is no associated Handle, and the
    //        index of the cell in [slots] is in [freelist].
    //      - Some, in which case there is exactly one associated Handle,
    //        and the index of the cell in [slots] is not in [freelist].
    //
    // Between 1. and 2., given a Handle, the corresponding StableCell must be
    // Some, and we can create a pointer to the contents. Given 0., we can
    // allow interior mutability to the contents.
    //
    // TODO(george) This isn't sound because two instances of Allocator could hand out
    // identical handles, allowing two threads to access the contents of one instance of the
    // Allocator. We can either hide Allocator and make there statically be only one instance
    // of it for a given type, or make Handles unforgeably unique across allocator instances.
    // Given the performance sensitivity and special-casedness of this code, I'd lean towards
    // the former.
    pub struct Inner<T> {
        slots: Vec<StableCell<T>>,
        freelist: Vec<usize>,
    }

    impl<T> Inner<T> {
        pub fn new() -> Self {
            Self {
                slots: Vec::new(),
                freelist: Vec::new(),
            }
        }

        pub fn alloc(&mut self, t: T) -> Handle {
            let handle = self.create_handle();
            *self.slots[handle.idx] = Some(RefCell::new(t));
            handle
        }

        pub fn free(&mut self, handle: Handle) -> T {
            let t = self.slots[handle.idx].take().unwrap().into_inner();
            self.drop_handle(handle);
            t
        }

        pub fn get(&mut self, handle: &Handle) -> core::ptr::NonNull<RefCell<T>> {
            let slot: &mut Option<RefCell<T>> = &mut self.slots[handle.idx];
            let refcell_ref = match slot {
                Some(r) => &mut *r,
                None => panic!("invalid Handle"),
            };
            let mut_refcell_ptr = refcell_ref as *mut _;
            core::ptr::NonNull::new(mut_refcell_ptr).unwrap()
        }

        fn create_handle(&mut self) -> Handle {
            let idx = match self.freelist.pop() {
                Some(idx) => idx,
                None => {
                    let idx = self.slots.len();
                    self.slots.push(Box::new(None));
                    idx
                }
            };
            Handle::new(idx)
        }

        fn drop_handle(&mut self, handle: Handle) {
            self.freelist.push(handle.idx)
        }
    }
}
