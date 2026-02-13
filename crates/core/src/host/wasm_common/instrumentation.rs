//! This file defines `CallSpan` and `CallSpanStart`,
//! which describe the time that a module spends executing "host calls,"
//! the WASM analog to syscalls.
//!
//! We can collect `CallSpan` timing data for various parts of host-call execution:
//! - `WasmInstanceEnv`, responsible for direct communication between WASM and the host.
//! - `InstanceEnv`, responsible for calls into the relational DB.
//! - Others?
//!
//! The instrumentation has non-negligible overhead,
//! so we enable it on a per-component basis with `cfg` attributes.
//! This is accomplished by defining two interchangeable modules,
//! `noop` and `op`, both of which implement the call-span interface.
//! `noop` does nothing.
//! `op` uses `std::time::Instant` and `std::time::Duration` to capture timings.
//! Components which use the time-span interface will conditionally import one of the two modules, like:
//! ```no-run
//! #[cfg(feature = "spacetimedb-wasm-instance-times)]
//! use instrumentation::op as span;
//! #[cfg(not(feature = "spacetimedb-wasm-instance-times)]
//! use instrumentation::noop as span;
//! ```

use std::time::{Duration, Instant};

use enum_map::{enum_map, EnumMap};

use crate::host::AbiCall;

#[allow(unused)]
pub mod noop {
    use crate::host::AbiCall;

    use super::*;

    pub struct CallSpanStart;

    impl CallSpanStart {
        pub fn new(_call: AbiCall) -> Self {
            Self
        }

        pub fn end(self) -> CallSpan {
            CallSpan
        }
    }

    pub struct CallSpan;

    pub fn record_span(_call_times: &mut CallTimes, _span: CallSpan) {}
}

#[allow(unused)]
pub mod op {
    use crate::host::AbiCall;

    use super::*;

    pub struct CallSpanStart {
        call: AbiCall,
        start: Instant,
    }

    impl CallSpanStart {
        pub fn new(call: AbiCall) -> Self {
            let start = Instant::now();
            Self { call, start }
        }

        pub fn end(self) -> CallSpan {
            let call = self.call;
            let duration = self.start.elapsed();
            CallSpan { call, duration }
        }
    }

    #[derive(Debug)]
    pub struct CallSpan {
        pub(super) call: AbiCall,
        pub(super) duration: Duration,
    }

    pub fn record_span(times: &mut CallTimes, span: CallSpan) {
        times.span(span)
    }
}

#[derive(Debug)]
/// Associates each `AbiCall` tag with a cumulative total `Duration` spent within that call.
pub struct CallTimes {
    times: EnumMap<AbiCall, Duration>,
}

impl Default for CallTimes {
    fn default() -> Self {
        Self::new()
    }
}

impl CallTimes {
    /// Create a new timing structure, with times for all calls set to zero.
    pub fn new() -> Self {
        let times = enum_map! { _ => Duration::ZERO };
        Self { times }
    }

    /// Track a particular `CallSpan` by adding its duration to the
    /// associated `AbiCall`'s timing information.
    pub fn span(&mut self, span: op::CallSpan) {
        self.times[span.call] += span.duration;
    }

    pub fn sum(&self) -> Duration {
        self.times.values().sum()
    }

    /// Taking the record of call times gives a copy of the
    /// current values and resets the values to zero.
    ///
    /// WasmInstanceEnv::finish_reducer (and other future per-reducer-call metrics)
    /// will `take`` the CallTimes after running a reducer and report the taken times,
    /// leaving a fresh zeroed CallTimes for the next reducer invocation.
    pub fn take(&mut self) -> CallTimes {
        std::mem::take(self)
    }
}

#[cfg(not(feature = "spacetimedb-wasm-instance-env-times"))]
pub use noop as span;
#[cfg(feature = "spacetimedb-wasm-instance-env-times")]
pub use op as span;
