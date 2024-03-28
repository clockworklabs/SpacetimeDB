mod page;
mod reader;
mod writer;

use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

pub use page::{BLOCK_SIZE, PAGE_SIZE};
pub use reader::PagedReader;
pub use writer::PagedWriter;

pub fn open_file(path: impl AsRef<Path>, opts: &mut OpenOptions) -> io::Result<File> {
    crate::repo::fs::open(path, opts, true, false)
}

pub trait WriteAt {
    fn write_at(&mut self, buf: &[u8], offset: u64) -> io::Result<usize>;
    fn write_all_at(&mut self, buf: &[u8], offset: u64) -> io::Result<()>;
}

#[cfg(unix)]
impl WriteAt for File {
    fn write_at(&mut self, buf: &[u8], offset: u64) -> io::Result<usize> {
        std::os::unix::fs::FileExt::write_at(self, buf, offset)
    }

    fn write_all_at(&mut self, buf: &[u8], offset: u64) -> io::Result<()> {
        std::os::unix::fs::FileExt::write_all_at(self, buf, offset)
    }
}

#[cfg(windows)]
impl WriteAt for File {
    fn write_at(&mut self, buf: &[u8], offset: u8) -> io::Result<usize> {
        std::os::windows::fs::FileExt::seek_write(buf, offset)
    }

    fn write_all_at(&mut self, mut buf: &[u8], mut offset: u8) -> io::Result<()> {
        while !buf.is_empty() {
            match self.write_at(buf, offset) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "failed to write whole buffer")),
                Ok(n) => {
                    buf = &buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl WriteAt for Vec<u8> {
    fn write_at(&mut self, buf: &[u8], offset: u64) -> io::Result<usize> {
        let offset = offset as usize;
        let needed = offset + buf.len();
        if needed > self.len() {
            self.resize(needed, 0);
        }
        self[offset..offset + buf.len()].copy_from_slice(buf);

        Ok(buf.len())
    }

    fn write_all_at(&mut self, buf: &[u8], offset: u64) -> io::Result<()> {
        self.write_at(buf, offset).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_at_vec_fills_with_zeroes_if_written_past_len() {
        let mut v = Vec::new();
        v.write_at(&[42; 512], 512).unwrap();
        assert_eq!(&v, &[[0; 512], [42; 512]].concat());
    }

    #[test]
    fn write_at_vec_overwrites_already_initialized_range() {
        let mut v = Vec::new();
        v.write_at(&[42; 512], 512).unwrap();
        v.write_at(&[41; 512], 0).unwrap();
        assert_eq!(&v, &[[41; 512], [42; 512]].concat());
    }

    #[test]
    fn write_at_vec_extends_past_len() {
        let mut v = Vec::new();
        v.write_at(&[42; 512], 512).unwrap();
        v.write_at(&[41; 512], 0).unwrap();
        v.write_at(&[43; 512], 1024).unwrap();
        assert_eq!(&v, &[[41; 512], [42; 512], [43; 512]].concat());
    }

    #[test]
    fn write_at_vec_overwrites_and_extends() {
        let mut v = Vec::new();
        v.write_at(&[42; 512], 512).unwrap();
        v.write_at(&[41; 512], 0).unwrap();
        v.write_at(&[43; 512], 1024).unwrap();
        v.write_at(&[44; 2048], 512).unwrap();
        assert_eq!(&v, &[[41; 512].as_slice(), [44; 2048].as_slice()].concat());
    }
}
