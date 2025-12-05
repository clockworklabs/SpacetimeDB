use core::mem::{ManuallyDrop, MaybeUninit};
use crossbeam_queue::ArrayQueue;
use std::sync::{Arc, Weak};
use wasmtime::{StackCreator, StackMemory};
use wasmtime_internal_fiber::FiberStack;

pub struct PoolingStackCreator {
    /// The actual pool of stacks.
    pool: ArrayQueue<FiberStack>,
    /// A weak reference to `self` that can be cloned
    /// and put into a `PooledFiberStack` which uses `weak` on drop.
    /// We do it self-referentially so that we can avoid another indirection.
    weak: MaybeUninit<Weak<PoolingStackCreator>>,
}

struct PooledFiberStack {
    stack: ManuallyDrop<FiberStack>,
    /// A weak reference to the pool so that `self.stack`
    /// can be returned to the pool on drop.
    weak: Weak<PoolingStackCreator>,
}

impl Drop for PooledFiberStack {
    fn drop(&mut self) {
        // SAFETY: `self.stack` is never used again.
        let stack = unsafe { ManuallyDrop::take(&mut self.stack) };

        let Some(pool) = self.weak.upgrade() else {
            return;
        };

        let _ = pool.pool.push(stack);
    }
}

/// SAFETY: The implementation forwards to `FiberStack as StackMemory`,
/// which wasmtime promises to be sound.
unsafe impl StackMemory for PooledFiberStack {
    fn top(&self) -> *mut u8 {
        self.stack.top().unwrap()
    }

    fn range(&self) -> std::ops::Range<usize> {
        self.stack.range().unwrap()
    }

    fn guard_range(&self) -> std::ops::Range<*mut u8> {
        self.stack.guard_range().unwrap()
    }
}

// SAFETY: Stacks created in `new_stack`
// are never used outside of a wasmtime instance
// and are not modified elsewhere.
unsafe impl StackCreator for PoolingStackCreator {
    fn new_stack(&self, size: usize, zeroed: bool) -> anyhow::Result<Box<dyn wasmtime::StackMemory>, anyhow::Error> {
        // SAFETY: `self.weak` is fully initialized whenever `new_stack` is called.
        let weak = unsafe { self.weak.assume_init_ref() }.clone();

        // Either take the stack from the pool
        // or fall back to creating a new one.
        let stack = weak
            .upgrade()
            .and_then(|pool| pool.pool.pop().map(Ok))
            .unwrap_or_else(|| FiberStack::new(size, zeroed))?;

        // Ship it.
        let stack = ManuallyDrop::new(stack);
        Ok(Box::new(PooledFiberStack { stack, weak }))
    }
}

impl PoolingStackCreator {
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|weak| {
            let pool = ArrayQueue::new(100);
            let weak = MaybeUninit::new(weak.clone());
            Self { pool, weak }
        })
    }
}
