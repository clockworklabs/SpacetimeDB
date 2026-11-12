/// TODO(this PR): docs
pub struct LogStopwatch {
    #[cfg(target_arch = "wasm32")]
    stopwatch_id: u32,
}

impl LogStopwatch {
    #[cfg(target_arch = "wasm32")]
    pub fn new(name: &str) -> Self {
        let name = name.as_bytes();
        let id = unsafe { spacetimedb_bindings_sys::raw::console_timer_start(name.as_ptr(), name.len()) };
        Self { stopwatch_id: id }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(_name: &str) -> Self {
        Self {}
    }

    pub fn end(self) {
        // just drop self
    }
}

#[cfg(target_arch = "wasm32")]
impl std::ops::Drop for LogStopwatch {
    fn drop(&mut self) {
        unsafe {
            spacetimedb_bindings_sys::raw::console_timer_end(self.stopwatch_id);
        }
    }
}
