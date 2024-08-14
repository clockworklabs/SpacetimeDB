use std::{
    collections::{btree_map, BTreeMap},
    io,
    ops::DerefMut as _,
    sync::{Arc, RwLock},
};

use crate::segment::FileLike;

use super::Repo;

type SharedLock<T> = Arc<RwLock<T>>;
type SharedBytes = SharedLock<Vec<u8>>;

/// A log segment backed by a `Vec<u8>`.
///
/// Note that this is not a faithful model of a file, as safe Rust requires to
/// protect the buffer with a lock. This means that pathological situations
/// arising from concurrent read/write access of a file are impossible to occur.
#[derive(Clone, Debug, Default)]
pub struct Segment {
    pos: u64,
    buf: SharedBytes,
}

impl Segment {
    pub fn len(&self) -> usize {
        self.buf.read().unwrap().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl From<SharedBytes> for Segment {
    fn from(buf: SharedBytes) -> Self {
        Self { pos: 0, buf }
    }
}

impl FileLike for Segment {
    fn fsync(&self) -> io::Result<()> {
        Ok(())
    }

    fn ftruncate(&self, size: u64) -> io::Result<()> {
        let mut inner = self.buf.write().unwrap();
        inner.resize(size as usize, 0);
        Ok(())
    }
}

impl io::Write for Segment {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.buf.write().unwrap();
        // Piggyback on unsafe code in Cursor
        let mut cursor = io::Cursor::new(inner.deref_mut());
        cursor.set_position(self.pos);
        let sz = cursor.write(buf)?;
        self.pos = cursor.position();

        Ok(sz)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for Segment {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let inner = self.buf.read().unwrap();
        let len = self.pos.min(inner.len() as u64);
        let n = io::Read::read(&mut &inner[(len as usize)..], buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl io::Seek for Segment {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let (base_pos, offset) = match pos {
            io::SeekFrom::Start(n) => {
                self.pos = n;
                return Ok(n);
            }
            io::SeekFrom::End(n) => (self.len() as u64, n),
            io::SeekFrom::Current(n) => (self.pos, n),
        };
        match base_pos.checked_add_signed(offset) {
            Some(n) => {
                self.pos = n;
                Ok(n)
            }
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowing position",
            )),
        }
    }
}

/// In-memory implementation of [`Repo`].
#[derive(Clone, Debug, Default)]
pub struct Memory(SharedLock<BTreeMap<u64, SharedBytes>>);

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Repo for Memory {
    type Segment = Segment;

    fn create_segment(&self, offset: u64) -> io::Result<Self::Segment> {
        let mut inner = self.0.write().unwrap();
        match inner.entry(offset) {
            btree_map::Entry::Occupied(entry) => {
                if entry.get().read().unwrap().len() == 0 {
                    Ok(Segment::from(Arc::clone(entry.get())))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("segment {offset} already exists"),
                    ))
                }
            }
            btree_map::Entry::Vacant(entry) => {
                let segment = entry.insert(Default::default());
                Ok(Segment::from(Arc::clone(segment)))
            }
        }
    }

    fn open_segment(&self, offset: u64) -> io::Result<Self::Segment> {
        let inner = self.0.read().unwrap();
        let Some(buf) = inner.get(&offset) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("segment {offset} does not exist"),
            ));
        };
        Ok(Segment::from(Arc::clone(buf)))
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        let mut inner = self.0.write().unwrap();
        if inner.remove(&offset).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("segment {offset} does not exist"),
            ));
        }

        Ok(())
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        Ok(self.0.read().unwrap().keys().copied().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, Write};

    #[test]
    fn segment_read_write_seek() {
        let mut segment = Segment::default();
        segment.write_all(b"alonso").unwrap();

        segment.seek(io::SeekFrom::Start(0)).unwrap();
        let mut buf = [0; 6];
        segment.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"alonso");

        segment.seek(io::SeekFrom::Start(2)).unwrap();
        let n = segment.read(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"onso");

        segment.seek(io::SeekFrom::Current(-4)).unwrap();
        let n = segment.read(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"onso");

        segment.seek(io::SeekFrom::End(-3)).unwrap();
        let n = segment.read(&mut buf).unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[0..3], b"nso");
    }
}
