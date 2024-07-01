use std::io::{self, Write};

use log::warn;

use crate::segment::FileLike;

use super::{
    page::{Page, BLOCK_SIZE, PAGE_SIZE},
    WriteAt,
};

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
pub struct PagedWriter<F: WriteAt> {
    file: F,
    file_pos: u64,
    panicked: bool,

    page: Page,
}

impl<F: WriteAt> PagedWriter<F> {
    /// Create a new `PagedWriter` for the given `file`.
    ///
    /// Writing will start at the beginning of the file.
    pub fn new(file: F) -> Self {
        Self::from_raw_parts(file, 0, Page::new())
    }

    /// Create a new `PagedWriter` from its constituent parts.
    ///
    /// This allows to adjust the file position, and reuse [`Page`]s.
    pub(crate) const fn from_raw_parts(file: F, file_pos: u64, page: Page) -> Self {
        Self {
            file,
            file_pos,
            panicked: false,
            page,
        }
    }

    /// Flush the buffer to storage at the current position, _iff_ the buffer is
    /// full.
    fn flush_aligned(&mut self) -> io::Result<()> {
        if !self.page.is_full() {
            return Ok(());
        }

        self.panicked = true;
        self.file.write_all_at(self.page.buf(), self.file_pos)?;
        self.panicked = false;

        self.file_pos += PAGE_SIZE as u64;
        self.page.reset();

        Ok(())
    }

    /// Flush the buffer to storage at the current position.
    ///
    /// If the buffer is not full, flush up to the next [`BLOCK_SIZE`] boundary,
    /// adding padding if necessary.
    ///
    /// After the write succeeded, the file and buffer positions are rewound to
    /// the _previous_ [`BLOCK_SIZE`] boundary.
    fn flush_all(&mut self) -> io::Result<()> {
        self.flush_aligned()?;

        if !self.page.is_empty() {
            let pos = self.page.pos();
            let next_block = self.page.next_block_offset();

            // Pad block with zeroes.
            self.page.buf_mut()[pos..next_block].fill(0);

            // Write out up to and including the padded block.
            self.panicked = true;
            self.file.write_all_at(&self.page.buf()[..next_block], self.file_pos)?;
            self.panicked = false;

            let prev_block = next_block.saturating_sub(BLOCK_SIZE);
            // If `pos` wasn't on a block boundary, move the last block to the
            // start of the buffer.
            if next_block != pos && prev_block > 0 {
                self.page.buf_mut().copy_within(prev_block..next_block, 0);
            }
            // Adjust the positions: `prev_block` bytes were flushed.
            self.page.set_pos(pos - prev_block);
            self.file_pos += prev_block as u64;
        }

        Ok(())
    }
}

impl<F: WriteAt + FileLike> PagedWriter<F> {
    /// Flush all buffered data and sync all OS-internal metadata to disk.
    pub fn sync_all(&mut self) -> io::Result<()> {
        self.flush_all()?;
        self.file.fsync()
    }

    /// Flush all buffered data and sync it to disk.
    ///
    /// Does not sync metadata unless needed for a subsequent data retrieval
    /// (see `man 2 fdatasync`).
    #[allow(unused)]
    pub fn sync_data(&mut self) -> io::Result<()> {
        self.flush_all()?;
        self.file.fdatasync()
    }
}

impl<F: WriteAt> Write for PagedWriter<F> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut wrote = 0;
        let mut buf = buf;

        while !buf.is_empty() {
            let (chunk, rest) = buf.split_at(self.page.spare_capacity().min(buf.len()));
            self.page.copy_from_slice(chunk);
            wrote += chunk.len();
            self.flush_aligned()?;
            buf = rest;
        }

        Ok(wrote)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.flush_all()
    }
}

impl<F: WriteAt> Drop for PagedWriter<F> {
    fn drop(&mut self) {
        if !self.panicked {
            if let Err(e) = self.flush_all() {
                warn!("failed to flush on drop: {e}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_flushes_aligned() {
        let mut writer = PagedWriter::new(Vec::new());
        writer.write_all(&[42; 4096]).unwrap();
        writer.write_all(&[1; 512]).unwrap();

        assert_eq!(&writer.file, &[42; 4096])
    }

    #[test]
    fn flush_flushes_all_with_padding() {
        let mut writer = PagedWriter::new(Vec::new());
        writer.write_all(&[42; 5000]).unwrap();
        writer.flush().unwrap();

        assert_eq!(
            &writer.file,
            [[42; 5000].as_slice(), [0; 120].as_slice()].concat().as_slice()
        );
    }

    #[test]
    fn write_after_padded_flush_overwrites_padding() {
        let mut writer = PagedWriter::new(Vec::new());

        writer.write_all(&[42; 4000]).unwrap();
        writer.flush().unwrap();
        assert_eq!(
            &writer.file,
            [[42; 4000].as_slice(), [0; 96].as_slice()].concat().as_slice()
        );

        writer.write_all(&[43; 96]).unwrap();
        writer.flush_all().unwrap();
        assert_eq!(writer.file.len(), 4096);
        assert_eq!(
            &writer.file,
            [[42; 4000].as_slice(), [43; 96].as_slice()].concat().as_slice()
        );
    }
}
