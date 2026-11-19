//! A global allocator for tests that counts each thread's live heap bytes,
//! so a test can measure the peak memory an operation allocates on its thread.
//!
//! Only allocations made on the calling thread are counted.
//! Work handed to other threads is not seen.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

thread_local! {
    // Signed, because a thread can free memory that another thread allocated.
    static LIVE: Cell<i64> = const { Cell::new(0) };
    static PEAK: Cell<i64> = const { Cell::new(0) };
}

fn grow(bytes: usize) {
    let bytes = i64::try_from(bytes).expect("an allocation's size fits in i64");
    // `try_with` fails while the thread's locals are being destroyed. Nothing measures then.
    let _ = LIVE.try_with(|live| {
        let now = live.get() + bytes;
        live.set(now);
        let _ = PEAK.try_with(|peak| peak.set(peak.get().max(now)));
    });
}

fn shrink(bytes: usize) {
    let bytes = i64::try_from(bytes).expect("an allocation's size fits in i64");
    let _ = LIVE.try_with(|live| live.set(live.get() - bytes));
}

// SAFETY: Every call forwards to `System` with the same arguments.
// The counting touches only `const` thread locals, which never allocate.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller upholds `GlobalAlloc::alloc`'s contract.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller upholds `GlobalAlloc::alloc_zeroed`'s contract.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller upholds `GlobalAlloc::dealloc`'s contract.
        unsafe { System.dealloc(ptr, layout) };
        shrink(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The caller upholds `GlobalAlloc::realloc`'s contract.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            grow(new_size);
            shrink(layout.size());
        }
        new
    }
}

/// Runs `f` and returns its result with the most heap bytes it held on this thread at once,
/// above what the thread held when `f` started.
pub(crate) fn peak_heap_bytes<R>(f: impl FnOnce() -> R) -> (R, u64) {
    let start = LIVE.with(Cell::get);
    PEAK.with(|peak| peak.set(start));
    let result = f();
    let peak = PEAK.with(Cell::get);
    let above = u64::try_from(peak - start).expect("the peak is at least the starting level");
    (result, above)
}
