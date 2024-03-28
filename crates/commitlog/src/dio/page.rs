use crate::buf;

/// The (assumed) size of an OS memory page: 4096 bytes.
pub const PAGE_SIZE: usize = 4096;

/// The (assumed) size of a logical device block.
///
/// Under Linux, this is the value returned by `blockdev --getss`.
///
/// Although the value may differ across machines, filesystems or OSs, we
/// currently assume that it is a safe value to align I/O buffers to.
pub const BLOCK_SIZE: usize = 512;

/// A buffer of size [`PAGE_SIZE`], aligned to [`BLOCK_SIZE`].
///
/// The memory of the buffer is intended to be reused by manipulating its
/// (write) position (similar to a cursor).
#[derive(Debug)]
pub(crate) struct Page {
    buf: buf::Aligned<PAGE_SIZE, BLOCK_SIZE>,
    pos: usize,
}

impl Page {
    /// Create a new page.
    pub fn new() -> Self {
        Self {
            buf: buf::Aligned::new(),
            pos: 0,
        }
    }

    /// The current position, i.e. up to which offset the buffer is considered
    /// filled.
    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Reset the current position to zero.
    #[inline]
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Set the current position to `pos`.
    ///
    /// The caller must ensure that `pos < self.buf.len()`.
    #[inline]
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Return the entire underlying buffer as a slice, regardless of position.
    #[inline]
    pub fn buf(&self) -> &[u8] {
        &self.buf
    }

    /// Return the entire underlying buffer as a mutable slice, regardless of
    /// position.
    ///
    /// The caller must ensure to update the position as necessary.
    #[inline]
    pub fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// Return the current position, rounded up to the next multiple of
    /// [`BLOCK_SIZE`].
    ///
    /// Never exceeds the length of the buffer.
    #[inline]
    pub fn next_block_offset(&self) -> usize {
        self.pos.next_multiple_of(BLOCK_SIZE).min(self.buf.len())
    }

    /// Copy the given slice to the internal buffer, starting from `self.pos()`.
    ///
    /// The position is updated with the length of the source slice.
    ///
    /// # Panics
    ///
    /// Panics if there is not enough space to copy `src`, i.e.
    /// `src.len() > self.spare_capacity()`.
    #[inline]
    pub fn copy_from_slice(&mut self, src: &[u8]) {
        self.buf[self.pos..self.pos + src.len()].copy_from_slice(src);
        self.pos += src.len();
    }

    /// `true` if the current position is zero.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.pos == 0
    }

    /// `true` if the buffer is full, shorthand for `self.spare_capacity() == 0`.
    pub const fn is_full(&self) -> bool {
        self.spare_capacity() == 0
    }

    /// Returns the number of bytes remaining in the buffer after `self.pos()`.
    pub const fn spare_capacity(&self) -> usize {
        self.buf.len() - self.pos
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}
