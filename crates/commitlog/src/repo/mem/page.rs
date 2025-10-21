use std::slice::SliceIndex;

pub const PAGE_SIZE: usize = 4096;

#[derive(Debug)]
pub struct Page {
    filled: usize,
    buf: [u8; PAGE_SIZE],
}

impl Page {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            filled: 0,
            buf: [0; PAGE_SIZE],
        }
    }

    pub fn remaining(&self) -> usize {
        PAGE_SIZE - self.filled
    }

    pub fn len(&self) -> usize {
        self.filled
    }

    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }

    pub fn modify_byte_at(&mut self, pos: usize, f: impl FnOnce(u8) -> u8) {
        self.buf[pos] = f(self.buf[pos])
    }

    pub fn copy_from_slice(&mut self, buf: &[u8]) {
        self.buf[self.filled..self.filled + buf.len()].copy_from_slice(buf);
        self.filled += buf.len();
    }

    pub fn slice<I>(&self, range: I) -> &I::Output
    where
        I: SliceIndex<[u8]>,
    {
        self.buf.get(range).expect("range out of bounds")
    }

    pub fn zeroize(&mut self, pos: usize) {
        self.buf[pos..].fill(0);
        self.filled = pos;
    }
}
