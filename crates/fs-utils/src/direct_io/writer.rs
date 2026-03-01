use std::io;

use super::page::Page;

/// A buffered writer using an aligned buffer internally.
///
/// Similar to [`io::BufWriter`], but suitable for files opened using `O_DIRECT`
/// or a platform equivalent, due to the alignment.
///
/// # Flushing behaviour
///
/// [`io::Write::write`] calls will only flush the buffer when it is full.
/// [`io::Write::flush`] calls, however, will flush the buffer up to the next
/// [`BLOCK_SIZE`] boundary, padding the data with zeroes if necessary.
///
/// This is done so that partial writes to the underlying storage can be
/// detected on the application layer, and retried if appropriate. It is also
/// necessary to preserve `fsync` semantics: a [`PagedWriter`] replaces the OS
/// page cache, where the latter is flushed to the device when `fsync` is called.
///
/// After a flush of an underfull buffer, the file and page positions are
/// rewound to the _previous_ [`BLOCK_SIZE`] boundary, such that the subsequent
/// write will overwrite the padding if more data has been added to the writer.
///
/// Dropping a [`PagedWriter`] will attempt to flush all data, but will not sync
/// it.
#[derive(Debug)]
pub struct AlignedBufWriter<W: io::Write> {
    inner: W,
    page: Page,
}

impl<W: io::Write> AlignedBufWriter<W> {
    /// Create a new `AlignedBufWriter` wrapping the `inner` writer.
    pub fn new(inner: W) -> Self {
        Self::from_raw_parts(inner, Page::new())
    }

    /// Create a new `AlignedBufWriter` from its constituent parts.
    ///
    /// This allows to reuse [`Page`]s.
    pub const fn from_raw_parts(inner: W, page: Page) -> Self {
        Self { inner, page }
    }

    /// Get a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
}

impl<F: io::Write> io::Write for AlignedBufWriter<F> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut wrote = 0;
        let mut buf = buf;

        while !buf.is_empty() {
            let (chunk, rest) = buf.split_at(self.page.spare_capacity().min(buf.len()));
            self.page.copy_from_slice(chunk);
            if self.page.is_full() {
                self.flush()?;
            }
            wrote += chunk.len();
            buf = rest;
        }

        Ok(wrote)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        let pos = self.page.pos();
        let next_block = self.page.next_block_offset();

        // Pad with zeroes.
        self.page.buf_mut()[pos..next_block].fill(0);
        let buf = &self.page.buf()[..next_block];
        let len = buf.len();

        self.inner.write_all(buf)?;
        if pos + len > self.page.capacity() {
            self.page.reset();
        } else {
            self.page.set_pos(pos + len);
        }
        self.inner.flush()?;

        Ok(())
    }
}

impl<F: io::Write> Drop for AlignedBufWriter<F> {
    fn drop(&mut self) {
        let _ = io::Write::flush(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn write_flushes_aligned() {
        let mut writer = AlignedBufWriter::new(Vec::new());
        writer.write_all(&[42; 4096]).unwrap();
        writer.write_all(&[1; 512]).unwrap();

        assert_eq!(&writer.inner, &[42; 4096])
    }

    #[test]
    fn flush_flushes_all_with_padding() {
        let mut writer = AlignedBufWriter::new(Vec::new());
        writer.write_all(&[42; 5000]).unwrap();
        writer.flush().unwrap();

        assert_eq!(
            &writer.inner,
            [[42; 5000].as_slice(), [0; 120].as_slice()].concat().as_slice()
        );
    }
}
