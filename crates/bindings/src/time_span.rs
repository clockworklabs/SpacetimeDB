pub struct Span {
    span_id: u32,
}

impl Span {
    pub fn start(name: &str) -> Self {
        let name = name.as_bytes();
        let id = unsafe { spacetimedb_bindings_sys::raw::_span_start(name.as_ptr(), name.len()) };
        Self { span_id: id }
    }

    pub fn end(self) {
        // just drop self
    }
}

impl std::ops::Drop for Span {
    fn drop(&mut self) {
        unsafe {
            spacetimedb_bindings_sys::raw::_span_end(self.span_id);
        }
    }
}
