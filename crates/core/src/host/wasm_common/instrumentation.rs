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

use enum_map::{enum_map, Enum, EnumMap};

#[allow(unused)]
pub mod noop {
    use super::*;

    pub struct CallSpanStart;

    impl CallSpanStart {
        pub fn new(_call: Call) -> Self {
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
    use super::*;

    pub struct CallSpanStart {
        call: Call,
        start: Instant,
    }

    impl CallSpanStart {
        pub fn new(call: Call) -> Self {
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
        pub(super) call: Call,
        pub(super) duration: Duration,
    }

    pub fn record_span(times: &mut CallTimes, span: CallSpan) {
        times.span(span)
    }
}

/// Tags for each call that a `WasmInstanceEnv` can make.
#[derive(Debug, Enum)]
pub enum Call {
    CancelReducer,
    ConsoleLog,
    CreateIndex,
    DeleteByColEq,
    GetTableId,
    Insert,
    IterByColEq,
    IterDrop,
    IterNext,
    IterStart,
    IterStartFiltered,
    ScheduleReducer,
}

#[derive(Debug)]
pub struct CallTimes {
    times: EnumMap<Call, Duration>,
}

impl CallTimes {
    /// Create a new timing structure, with times for all calls set to zero.
    pub fn new() -> Self {
        let times = enum_map! { _ => Duration::ZERO };
        Self { times }
    }

    /// Track a particular `CallSpan` by adding its duration to the
    /// associated `Call`'s timing information.
    pub fn span(&mut self, span: op::CallSpan) {
        self.times[span.call] += span.duration;
    }

    /// Taking the record of call times gives a copy of the
    /// current values and resets the values to zero.
    pub fn take(&mut self) -> CallTimes {
        std::mem::replace(self, Self::new())
    }
}
