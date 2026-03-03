use std::{
    cmp,
    io::{self, BufRead, Read, Seek},
};

use super::page::Page;

/// A buffered reader using an aligned buffer internally.
///
/// The alignment makes the reader suitable for files opened using `O_DIRECT`
/// or a platform equivalent.
///
/// Other than the alignment of the buffer, this is basically a stripped down
/// version of [`io::BufReader`], borrowing much of its code.
pub struct AlignedBufReader<R> {
    inner: R,

    page: Page,
    /// The number of bytes read during the last `fill_buf`.
    ///
    /// That is, `page.buf()[page.pos()..filled]` is the currently buffered,
    /// unconsumed data.
    filled: usize,
}

impl<R> AlignedBufReader<R> {
    /// Create a new [AlignedBufReader] wrapping the `inner` reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            page: Page::new(),
            filled: 0,
        }
    }

    pub const fn from_raw_parts(inner: R, page: Page, filled: usize) -> Self {
        Self { inner, page, filled }
    }

    pub fn into_raw_parts(self) -> (R, Page, usize) {
        (self.inner, self.page, self.filled)
    }
}

impl<R: Read> AlignedBufReader<R> {
    #[inline]
    fn consume_with(&mut self, amt: usize, mut visitor: impl FnMut(&[u8])) -> bool {
        if let Some(claimed) = self.page.buf()[self.page.pos()..self.filled].get(..amt) {
            visitor(claimed);
            self.consume(amt);
            true
        } else {
            false
        }
    }
}

impl<R: Read> Read for AlignedBufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut rem = self.fill_buf()?;
        let n = rem.read(buf)?;
        self.consume(n);

        Ok(n)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        if self.consume_with(buf.len(), |claimed| buf.copy_from_slice(claimed)) {
            return Ok(());
        }

        let mut buf = buf;
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }

        if !buf.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole page",
            ));
        }

        Ok(())
    }
}

impl<R: Read> BufRead for AlignedBufReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.page.pos() >= self.filled {
            let n = self.inner.read(self.page.buf_mut())?;
            self.page.reset();
            self.filled = n;
        }

        Ok(&self.page.buf()[self.page.pos()..self.filled])
    }

    fn consume(&mut self, amt: usize) {
        self.page.set_pos(cmp::min(self.page.pos() + amt, self.filled));
    }
}

impl<R: Seek> Seek for AlignedBufReader<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.page.reset();
        self.filled = 0;
        self.inner.seek(pos)
    }
}
