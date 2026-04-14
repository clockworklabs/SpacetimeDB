use core::mem::ManuallyDrop;
use crossbeam_queue::ArrayQueue;
use std::sync::{Arc, Weak};
use wasmtime::{StackCreator, StackMemory};
use wasmtime_internal_fiber::FiberStack;

/// The stack size for async stacks.
pub const ASYNC_STACK_SIZE: usize = 2 << 20;

pub struct PoolingStackCreator {
    /// The actual pool of stacks.
    pool: ArrayQueue<FiberStack>,
    /// A weak reference to `self` that can be cloned
    /// and put into a `PooledFiberStack` which uses `weak` on drop.
    /// We do it self-referentially so that we can avoid another indirection.
    weak: Weak<PoolingStackCreator>,
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

const UNIX_SOME: &str = "FiberStack on unix always returns `Some(_)`";

/// SAFETY: The implementation forwards to `FiberStack as StackMemory`,
/// which wasmtime promises to be sound.
unsafe impl StackMemory for PooledFiberStack {
    fn top(&self) -> *mut u8 {
        self.stack.top().expect(UNIX_SOME)
    }

    fn range(&self) -> std::ops::Range<usize> {
        self.stack.range().expect(UNIX_SOME)
    }

    fn guard_range(&self) -> std::ops::Range<*mut u8> {
        self.stack.guard_range().expect(UNIX_SOME)
    }
}

// SAFETY: Stacks created in `new_stack`
// are never used outside of a wasmtime instance
// and are not modified elsewhere.
unsafe impl StackCreator for PoolingStackCreator {
    fn new_stack(&self, size: usize, zeroed: bool) -> anyhow::Result<Box<dyn wasmtime::StackMemory>, anyhow::Error> {
        assert_eq!(size, ASYNC_STACK_SIZE);

        // SAFETY: `self.weak` is fully initialized whenever `new_stack` is called.
        let weak = self.weak.clone();

        // Either take the stack from the pool
        // or fall back to creating a new one.
        let stack = weak
            .upgrade()
            .and_then(|pool| pool.pool.pop().map(Ok))
            .unwrap_or_else(|| FiberStack::new(ASYNC_STACK_SIZE, zeroed))?;

        // Ship it.
        let stack = ManuallyDrop::new(stack);
        Ok(Box::new(PooledFiberStack { stack, weak }))
    }
}

impl PoolingStackCreator {
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|weak| {
            let pool = ArrayQueue::new(100);
            let weak = weak.clone();
            Self { pool, weak }
        })
    }
}
