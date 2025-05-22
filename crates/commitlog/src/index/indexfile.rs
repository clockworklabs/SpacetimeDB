use std::{
    fs::{self, File},
    io,
    marker::PhantomData,
    mem,
};

use log::debug;
use memmap2::MmapMut;
use spacetimedb_paths::server::OffsetIndexFile;

use super::IndexError;
const KEY_SIZE: usize = mem::size_of::<u64>();
const ENTRY_SIZE: usize = KEY_SIZE + mem::size_of::<u64>();

/// A mutable representation of an index file using memory-mapped I/O.
///
/// `IndexFileMut` provides efficient read and write access to an index file, which stores
/// key-value pairs
/// Succesive key written should be sorted in ascending order, 0 is invalid-key value
#[derive(Debug)]
pub struct IndexFileMut<Key> {
    // A mutable memory-mapped buffer that represents the file contents.
    inner: MmapMut,
    /// The number of entries currently stored in the index file.
    num_entries: usize,

    _marker: PhantomData<Key>,
}

impl<Key: Into<u64> + From<u64>> IndexFileMut<Key> {
    pub fn create_index_file(path: &OffsetIndexFile, cap: u64) -> io::Result<Self> {
        path.open_file(File::options().write(true).read(true).create_new(true))
            .and_then(|file| {
                file.set_len(cap * ENTRY_SIZE as u64)?;
                let mmap = unsafe { MmapMut::map_mut(&file) }?;

                Ok(IndexFileMut {
                    inner: mmap,
                    num_entries: 0,
                    _marker: PhantomData,
                })
            })
            .or_else(|e| {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    debug!("Index file {} already exists", path.display());
                    Self::open_index_file(path, cap)
                } else {
                    Err(e)
                }
            })
    }

    pub fn open_index_file(path: &OffsetIndexFile, cap: u64) -> io::Result<Self> {
        let file = path.open_file(File::options().read(true).write(true))?;
        file.set_len(cap * ENTRY_SIZE as u64)?;
        let mmap = unsafe { MmapMut::map_mut(&file)? };

        let mut me = IndexFileMut {
            inner: mmap,
            num_entries: 0,
            _marker: PhantomData,
        };
        me.num_entries = me.num_entries().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(me)
    }

    pub fn delete_index_file(path: &OffsetIndexFile) -> io::Result<()> {
        fs::remove_file(path).map_err(Into::into)
    }

    // Searches for first 0-key, to count number of entries
    fn num_entries(&self) -> Result<usize, IndexError> {
        for index in 0.. {
            match self.index_lookup(index) {
                Ok((entry, _)) => {
                    if entry.into() == 0 {
                        return Ok(index);
                    }
                }
                Err(IndexError::OutOfRange) => return Ok(index),
                Err(e) => return Err(e),
            }
        }
        Ok(0)
    }

    /// Finds the 0 based index of the first key encountered that is just smaller than or equal to the given key.
    ///
    /// # Error
    ///
    /// - `IndexError::KeyNotFound`: If the key is smaller than the first entry key
    pub fn find_index(&self, key: Key) -> Result<(Key, u64), IndexError> {
        let key = key.into();

        let mut low = 0;
        let mut high = self.num_entries;

        while low < high {
            let mid = low + (high - low) / 2;
            let (mid_key, _) = self.index_lookup(mid)?;
            if mid_key.into() > key {
                high = mid;
            } else {
                low = mid;
            }

            if high - low == 1 {
                break;
            }
        }

        let low_key = self.index_lookup(low).map(|(k, _)| k.into())?;
        if low == 0 && key < low_key {
            return Err(IndexError::KeyNotFound);
        }
        // If found key is 0, return `KeyNotFound`
        if low_key == 0 {
            return Err(IndexError::KeyNotFound);
        }

        Ok((Key::from(low_key), low as u64))
    }

    /// Looks up the key-value pair at the specified index in the index file.
    /// # Errors
    ///
    /// - `IndexError::OutOfMemory`: If the index is out of memory range.
    fn index_lookup(&self, index: usize) -> Result<(Key, u64), IndexError> {
        let start = index * ENTRY_SIZE;
        if start + ENTRY_SIZE > self.inner.len() {
            return Err(IndexError::OutOfRange);
        }

        entry(&self.inner, start)
    }

    /// Returns the last key in the index file.
    /// Or 0 if no key is present
    fn last_key(&self) -> Result<u64, IndexError> {
        if self.num_entries == 0 {
            return Ok(0);
        }
        let start = (self.num_entries - 1) * ENTRY_SIZE;
        u64_from_le_bytes(&self.inner[start..start + KEY_SIZE])
    }

    // Return (key, value) pair of key just smaller or equal to given key
    ///
    /// # Error
    /// - `IndexError::KeyNotFound`: If the key is smaller than the first entry key
    pub fn key_lookup(&self, key: Key) -> Result<(Key, u64), IndexError> {
        let (_, idx) = self.find_index(key)?;
        self.index_lookup(idx as usize)
    }

    /// Appends a key-value pair to the index file.
    /// Successive calls to `append` must supply key in ascending order
    ///
    /// Errors
    /// - `IndexError::InvalidInput`: Either Key or Value is 0
    /// - `IndexError::OutOfMemory`: Append after index file is already full.
    pub fn append(&mut self, key: Key, value: u64) -> Result<(), IndexError> {
        let key = key.into();
        let last_key = self.last_key()?;
        if last_key >= key {
            return Err(IndexError::InvalidInput(last_key, key));
        }

        let start = self.num_entries * ENTRY_SIZE;
        if start + ENTRY_SIZE > self.inner.len() {
            return Err(IndexError::OutOfRange);
        }

        let key_bytes = key.to_le_bytes();
        let value_bytes = value.to_le_bytes();

        self.inner[start..start + KEY_SIZE].copy_from_slice(&key_bytes);
        self.inner[start + KEY_SIZE..start + ENTRY_SIZE].copy_from_slice(&value_bytes);
        self.num_entries += 1;
        Ok(())
    }

    /// Asynchronously flushes any pending changes to the index file
    ///
    /// Due to Async nature, `Ok(())` does not guarantee that the changes are flushed.
    /// an `Err` value indicates it definately did not succeed
    pub fn async_flush(&self) -> io::Result<()> {
        self.inner.flush_async()
    }

    /// Truncates the index file starting from the entry with a key greater than or equal to the given key.
    pub(crate) fn truncate(&mut self, key: Key) -> Result<(), IndexError> {
        let key = key.into();
        let (found_key, index) = self.find_index(Key::from(key))?;

        // If returned key is smalled than asked key, truncate from next entry
        self.num_entries = if found_key.into() == key {
            index as usize
        } else {
            index as usize + 1
        };

        let start = self.num_entries * ENTRY_SIZE;

        if start < self.inner.len() {
            self.inner[start..].fill(0);
        }

        self.inner.flush()?;

        Ok(())
    }

    /// Obtain an iterator over the entries of the index.
    pub fn entries(&self) -> Entries<Key> {
        Entries {
            mmap: &self.inner,
            pos: 0,
            max: self.num_entries * ENTRY_SIZE,
            _key: PhantomData,
        }
    }
}

impl<'a, K: Into<u64> + From<u64>> IntoIterator for &'a IndexFileMut<K> {
    type Item = Result<(K, u64), IndexError>;
    type IntoIter = Entries<'a, K>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries()
    }
}

impl<Key: Into<u64> + From<u64>> From<IndexFile<Key>> for IndexFileMut<Key> {
    fn from(IndexFile { inner }: IndexFile<Key>) -> Self {
        inner
    }
}

/// A wrapper over [`IndexFileMut`] to provide read-only access to the index file.
pub struct IndexFile<Key> {
    inner: IndexFileMut<Key>,
}

impl<Key: Into<u64> + From<u64>> IndexFile<Key> {
    pub fn open_index_file(path: &OffsetIndexFile) -> io::Result<Self> {
        let file = path.open_file(File::options().read(true).write(true))?;
        let mmap = unsafe { MmapMut::map_mut(&file)? };

        let mut inner = IndexFileMut {
            inner: mmap,
            num_entries: 0,
            _marker: PhantomData,
        };
        inner.num_entries = inner
            .num_entries()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Self { inner })
    }

    pub fn key_lookup(&self, key: Key) -> Result<(Key, u64), IndexError> {
        self.inner.key_lookup(key)
    }

    /// Obtain an iterator over the entries of the index.
    pub fn entries(&self) -> Entries<Key> {
        self.inner.entries()
    }
}

impl<K> AsMut<IndexFileMut<K>> for IndexFile<K> {
    fn as_mut(&mut self) -> &mut IndexFileMut<K> {
        &mut self.inner
    }
}

impl<'a, Key: Into<u64> + From<u64>> IntoIterator for &'a IndexFile<Key> {
    type Item = Result<(Key, u64), IndexError>;
    type IntoIter = Entries<'a, Key>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries()
    }
}

impl<Key: Into<u64> + From<u64>> From<IndexFileMut<Key>> for IndexFile<Key> {
    fn from(inner: IndexFileMut<Key>) -> Self {
        Self { inner }
    }
}

/// Iterator over the entries of an [`IndexFileMut`] or [`IndexFile`].
///
/// Yields pairs of `(K, u64)` or an error if an entry could not be decoded.
pub struct Entries<'a, K> {
    mmap: &'a [u8],
    pos: usize,
    max: usize,
    _key: PhantomData<K>,
}

impl<K: From<u64>> Iterator for Entries<'_, K> {
    type Item = Result<(K, u64), IndexError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.max {
            return None;
        }

        let item = entry(self.mmap, self.pos);
        if item.is_ok() {
            self.pos += ENTRY_SIZE;
        }
        Some(item)
    }
}

fn entry<K: From<u64>>(mmap: &[u8], start: usize) -> Result<(K, u64), IndexError> {
    let entry = &mmap[start..start + ENTRY_SIZE];
    let sz = mem::size_of::<u64>();
    let key = u64_from_le_bytes(&entry[..sz])?;
    let val = u64_from_le_bytes(&entry[sz..])?;

    Ok((key.into(), val))
}

fn u64_from_le_bytes(x: &[u8]) -> Result<u64, IndexError> {
    x.try_into()
        .map_err(|_| IndexError::InvalidFormat)
        .map(u64::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use pretty_assertions::assert_matches;
    use spacetimedb_paths::server::CommitLogDir;
    use spacetimedb_paths::FromPathUnchecked;
    use tempfile::TempDir;

    /// Create and fill index file with key as first `fill_till - 1` even numbers
    fn create_and_fill_index(cap: u64, fill_till: u64) -> Result<IndexFileMut<u64>, IndexError> {
        // Create a temporary directory for testing.
        // Dropping this at the end of the function is fine, as we're memory-
        // mapping the index file.
        let temp_dir = TempDir::new()?;
        create_and_fill_index_in(temp_dir.path(), cap, fill_till)
    }

    fn create_and_fill_index_in(dir: &Path, cap: u64, fill_till: u64) -> Result<IndexFileMut<u64>, IndexError> {
        // Create an index file
        let mut index_file = create_index_in(dir, cap)?;

        // Enter even number keys from 2
        for i in 1..fill_till {
            index_file.append(i * 2, i * 2 * 100)?;
        }

        Ok(index_file)
    }

    /// Create an index file in `dir`.
    ///
    /// Useful if `dir` is a temporary directory and should not be dropped.
    fn create_index_in(dir: &Path, cap: u64) -> io::Result<IndexFileMut<u64>> {
        let index_path = index_path(dir);
        IndexFileMut::create_index_file(&index_path, cap)
    }

    fn index_path(dir: &Path) -> OffsetIndexFile {
        CommitLogDir::from_path_unchecked(dir).index(0)
    }

    trait KeyLookup {
        type Key;
        fn key_lookup(&self, key: Self::Key) -> Result<(Self::Key, u64), IndexError>;
    }

    impl<K: Into<u64> + From<u64>> KeyLookup for IndexFileMut<K> {
        type Key = K;
        fn key_lookup(&self, key: Self::Key) -> Result<(Self::Key, u64), IndexError> {
            IndexFileMut::key_lookup(self, key)
        }
    }

    impl<K: Into<u64> + From<u64>> KeyLookup for IndexFile<K> {
        type Key = K;
        fn key_lookup(&self, key: Self::Key) -> Result<(Self::Key, u64), IndexError> {
            IndexFile::key_lookup(self, key)
        }
    }

    fn assert_key_lookup(index: &impl KeyLookup<Key = u64>) -> Result<(), IndexError> {
        // looking for exact match key
        assert_eq!(index.key_lookup(2)?, (2, 200));

        // Should fetch smaller key
        assert_eq!(index.key_lookup(5)?, (4, 400));

        // Key bigger than last entry should return last entry
        assert_eq!(index.key_lookup(100)?, (8, 800));

        // key smaller than 1st entry should return error
        assert!(index.key_lookup(1).is_err());

        Ok(())
    }

    #[test]
    fn test_empty_index_lookup_should_fail() -> Result<(), IndexError> {
        let index = create_index_in(TempDir::new().unwrap().path(), 100)?;
        assert_matches!(index.key_lookup(0), Err(IndexError::KeyNotFound));
        assert_matches!(index.key_lookup(10), Err(IndexError::KeyNotFound));
        Ok(())
    }

    #[test]
    fn test_key_lookup() -> Result<(), IndexError> {
        let index = create_and_fill_index(10, 5)?;
        assert_key_lookup(&index)
    }

    #[test]
    fn test_key_lookup_reopen() -> Result<(), IndexError> {
        let tmp = TempDir::new()?;
        create_and_fill_index_in(tmp.path(), 10, 5)?;

        // Re-open as mutable index.
        let index: IndexFileMut<_> = IndexFileMut::open_index_file(&index_path(tmp.path()), 10)?;
        assert_key_lookup(&index)
    }

    #[test]
    fn test_key_lookup_readonly() -> Result<(), IndexError> {
        let tmp = TempDir::new()?;
        create_and_fill_index_in(tmp.path(), 10, 5)?;

        // Re-open as read-only index.
        let index: IndexFile<u64> = IndexFile::open_index_file(&index_path(tmp.path()))?;
        assert_key_lookup(&index)
    }

    #[test]
    fn test_append() -> Result<(), IndexError> {
        // fill till one below capacity
        let mut index = create_and_fill_index(10, 10)?;

        assert_eq!(index.num_entries, 9);

        // append smaller than already appended key
        assert!(index.append(17, 300).is_err());

        // append duplicate key
        assert!(index.append(18, 500).is_err());

        // append to fill the capacty
        assert!(index.append(22, 500).is_ok());

        // Append after capacity should give error
        assert!(index.append(224, 600).is_err());

        Ok(())
    }

    #[test]
    fn test_truncate() -> Result<(), IndexError> {
        let mut index = create_and_fill_index(10, 9)?;

        assert_eq!(index.num_entries, 8);

        // Truncate last present entry
        index.truncate(16)?;
        assert_eq!(index.num_entries, 7);

        // Truncate from middle key entry
        // as key is not present, key with bigger entries should truncate
        index.truncate(9)?;
        assert_eq!(index.num_entries, 4);

        // Truncate from middle key entry
        // as key is not present, key with bigger entries should truncate
        index.truncate(9)?;
        assert_eq!(index.num_entries, 4);

        // Truncating from bigger key than already present must be no-op
        index.truncate(9)?;
        assert_eq!(index.num_entries, 4);

        Ok(())
    }

    #[test]
    fn test_close_open_index() -> Result<(), IndexError> {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new()?;
        let path = CommitLogDir::from_path_unchecked(temp_dir.path());
        let index_path = path.index(0);

        // Create an index file
        let mut index_file: IndexFileMut<u64> = IndexFileMut::create_index_file(&index_path, 100)?;

        for i in 1..10 {
            index_file.append(i * 2, i * 2 * 100)?;
        }

        assert_eq!(index_file.num_entries, 9);
        drop(index_file);

        let open_index_file: IndexFileMut<u64> = IndexFileMut::open_index_file(&index_path, 100)?;
        assert_eq!(open_index_file.num_entries, 9);
        assert_eq!(open_index_file.key_lookup(6)?, (6, 600));

        Ok(())
    }

    #[test]
    fn test_iterator_iterates() -> Result<(), IndexError> {
        let index = create_and_fill_index(100, 100)?;

        let expected = (1..100).map(|key| (key * 2, key * 2 * 100)).collect::<Vec<_>>();
        let entries = index.entries().collect::<Result<Vec<_>, _>>()?;
        assert_eq!(&entries, &expected);

        // `IndexFile` should yield the same result
        let index: IndexFile<u64> = index.into();
        let entries = index.entries().collect::<Result<Vec<_>, _>>()?;
        assert_eq!(&entries, &expected);

        Ok(())
    }

    #[test]
    fn test_iterator_yields_nothing_for_empty_index() -> Result<(), IndexError> {
        let index = create_and_fill_index(100, 0)?;
        let entries = index.entries().collect::<Result<Vec<_>, _>>()?;
        assert!(entries.is_empty());

        Ok(())
    }
}
