#[cfg(target_arch = "wasm32")]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

// `std::time::Instant::now` panics on `wasm32-unknown-unknown`. The portable
// datastore does not report production metrics, so zero-duration timings are
// sufficient for wasm module unit tests.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug)]
pub struct Instant;

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Self {
        Self
    }

    pub fn elapsed(&self) -> Duration {
        Duration::ZERO
    }
}
