pub struct LogStopwatch {
    stopwatch_id: u32,
}

impl LogStopwatch {
    pub fn new(name: &str) -> Self {
        let name = name.as_bytes();
        let id = unsafe { spacetimedb_bindings_sys::raw::console_timer_start(name.as_ptr(), name.len()) };
        Self { stopwatch_id: id }
    }

    pub fn end(self) {
        // just drop self
    }
}

impl std::ops::Drop for LogStopwatch {
    fn drop(&mut self) {
        unsafe {
            spacetimedb_bindings_sys::raw::console_timer_end(self.stopwatch_id);
        }
    }
}
