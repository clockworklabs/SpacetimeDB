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
