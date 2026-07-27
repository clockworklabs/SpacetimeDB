#[cfg(target_arch = "wasm32")]
use std::time::Duration;

use spacetimedb_lib::Timestamp;

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[allow(deprecated)]
pub fn now_timestamp() -> Timestamp {
    Timestamp::now()
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub fn now_timestamp() -> Timestamp {
    Timestamp::UNIX_EPOCH
}

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
