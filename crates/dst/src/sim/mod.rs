//! Local simulation shim for the DST crate.
//!
//! This module is deliberately small, but its executor shape follows madsim's:
//! futures are scheduled as runnable tasks and the ready queue is sampled by a
//! deterministic RNG instead of being driven by a package-level async runtime.

pub(crate) mod commitlog;
pub(crate) mod snapshot;
pub(crate) mod storage_faults;
pub mod time;

use std::{cell::RefCell, future::Future, time::Duration};

pub use spacetimedb_runtime::sim::{yield_now, Handle, JoinHandle, Node, NodeBuilder, NodeId, Rng};

thread_local! {
    static CURRENT_HANDLE: RefCell<Option<Handle>> = const { RefCell::new(None) };
}

struct CurrentHandleGuard {
    previous: Option<Handle>,
}

fn enter_current_handle(handle: Handle) -> CurrentHandleGuard {
    let previous = CURRENT_HANDLE.with(|slot| slot.replace(Some(handle)));
    CurrentHandleGuard { previous }
}

impl Drop for CurrentHandleGuard {
    fn drop(&mut self) {
        CURRENT_HANDLE.with(|slot| {
            let _ = slot.replace(self.previous.take());
        });
    }
}

pub(crate) fn current_handle() -> Option<Handle> {
    CURRENT_HANDLE.with(|slot| slot.borrow().clone())
}

const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(GAMMA);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

pub(crate) fn fork_seed(seed: u64, discriminator: u64) -> u64 {
    splitmix64(seed ^ discriminator.wrapping_mul(GAMMA))
}

/// DST-facing wrapper that keeps the top-level seed type local to this crate.
pub struct Runtime {
    inner: spacetimedb_runtime::sim::Runtime,
}

impl Runtime {
    pub fn new(seed: u64) -> anyhow::Result<Self> {
        Ok(Self {
            inner: spacetimedb_runtime::sim::Runtime::new(seed),
        })
    }

    pub fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        let _guard = enter_current_handle(self.inner.handle());
        spacetimedb_runtime::sim_std::block_on(&mut self.inner, future)
    }

    pub fn elapsed(&self) -> Duration {
        self.inner.elapsed()
    }

    pub fn handle(&self) -> Handle {
        self.inner.handle()
    }

    pub fn create_node(&self) -> NodeBuilder {
        self.inner.create_node()
    }

    pub fn pause(&self, node: NodeId) {
        self.inner.pause(node);
    }

    pub fn resume(&self, node: NodeId) {
        self.inner.resume(node);
    }

    pub fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn_on(node, future)
    }

    pub fn check_determinism<F>(seed: u64, make_future: fn() -> F) -> F::Output
    where
        F: Future + 'static,
        F::Output: Send + 'static,
    {
        spacetimedb_runtime::sim_std::check_determinism(seed, make_future)
    }

    pub fn check_determinism_with<M, F>(seed: u64, make_future: M) -> F::Output
    where
        M: Fn() -> F + Clone + Send + 'static,
        F: Future + 'static,
        F::Output: Send + 'static,
    {
        spacetimedb_runtime::sim_std::check_determinism(seed, make_future)
    }
}
#[allow(dead_code)]
pub(crate) fn decision_source(seed: u64) -> Rng {
    Rng::new(seed)
}
