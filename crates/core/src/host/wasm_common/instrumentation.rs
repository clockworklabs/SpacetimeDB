use std::time::{Duration, Instant};

use enum_map::{enum_map, Enum, EnumMap};

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
    call: Call,
    duration: Duration,
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
    pub fn span(&mut self, span: CallSpan) {
        self.times[span.call] += span.duration;
    }

    /// Taking the record of call times gives a copy of the
    /// current values and resets the values to zero.
    pub fn take(&mut self) -> CallTimes {
        std::mem::replace(self, Self::new())
    }
}
