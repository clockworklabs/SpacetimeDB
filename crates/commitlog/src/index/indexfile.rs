use std::{
    fs::{self, File},
    io,
    marker::PhantomData,
    mem,
    path::{Path, PathBuf},
};

use log::debug;
use memmap2::MmapMut;

use super::IndexError;
const OFFSET_INDEX_FILE_EXT: &str = ".stdb.ofs";
const KEY_SIZE: usize = mem::size_of::<u64>();
const ENTRY_SIZE: usize = KEY_SIZE + mem::size_of::<u64>();

/// Returns the offset index file path based on the root path and offset
pub fn offset_index_file_path(root: &Path, offset: u64) -> PathBuf {
    root.join(format!("{offset:0>20}{OFFSET_INDEX_FILE_EXT}"))
}

/// A mutable representation of an index file using memory-mapped I/O.
///
/// `IndexFileMut` provides efficient read and write access to an index file, which stores
/// key-value pairs
/// Succesive key written should be sorted in ascending order, 0 is invalid-key value
#[derive(Debug)]
pub struct IndexFileMut<Key: Into<u64> + From<u64>> {
    // A mutable memory-mapped buffer that represents the file contents.
    inner: MmapMut,
    /// The number of entries currently stored in the index file.
    num_entries: usize,

    _marker: PhantomData<Key>,
}

impl<Key: Into<u64> + From<u64>> IndexFileMut<Key> {
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
    // TODO: use binary search
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

        let entry = &self.inner[start..start + ENTRY_SIZE];

        let key = u64::from_le_bytes(
            entry[..mem::size_of::<u64>()]
                .try_into()
                .map_err(|_| IndexError::InvalidFormat)?,
        );
        let value = u64::from_le_bytes(
            entry[mem::size_of::<u64>()..]
                .try_into()
                .map_err(|_| IndexError::InvalidFormat)?,
        );

        Ok((Key::from(key), value))
    }

    /// Returns the last key in the index file.
    /// Or 0 if no key is present
    fn last_key(&self) -> Result<u64, IndexError> {
        if self.num_entries == 0 {
            return Ok(0);
        }
        let start = (self.num_entries - 1) * ENTRY_SIZE;
        let key_bytes: &[u8] = &self.inner[start..start + KEY_SIZE];
        let key = u64::from_le_bytes(key_bytes.try_into().map_err(|_| IndexError::InvalidFormat)?);
        Ok(key)
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
        if self.last_key()? >= key {
            return Err(IndexError::InvalidInput);
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
    pub fn truncate(&mut self, key: Key) -> Result<(), IndexError> {
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
}

pub fn create_index_file<Key: Into<u64> + From<u64>>(
    path: &Path,
    offset: u64,
    cap: u64,
) -> io::Result<IndexFileMut<Key>> {
    File::options()
        .write(true)
        .read(true)
        .create_new(true)
        .open(offset_index_file_path(path, offset))
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
                open_index_file(path, offset, cap)
            } else {
                Err(e)
            }
        })
}

pub fn open_index_file<Key: Into<u64> + From<u64>>(
    path: &Path,
    offset: u64,
    cap: u64,
) -> io::Result<IndexFileMut<Key>> {
    let file = File::options()
        .read(true)
        .write(true)
        .open(offset_index_file_path(path, offset))?;
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

pub fn delete_index_file(path: &Path, offset: u64) -> io::Result<()> {
    fs::remove_file(offset_index_file_path(path, offset)).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create and fill index file with key as first `fill_till - 1` even numbers
    fn create_and_fill_index(cap: u64, fill_till: u64) -> Result<IndexFileMut<u64>, IndexError> {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path().to_path_buf();

        // Create an index file
        let mut index_file: IndexFileMut<u64> = create_index_file(&path, 0, cap)?;

        // Enter even number keys from 2
        for i in 1..fill_till {
            index_file.append(i * 2, i * 2 * 100)?;
        }

        Ok(index_file)
    }

    #[test]
    fn test_key_lookup() -> Result<(), IndexError> {
        let index = create_and_fill_index(10, 5)?;

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
        let path = temp_dir.path().to_path_buf();

        // Create an index file
        let mut index_file: IndexFileMut<u64> = create_index_file(&path, 0, 100)?;

        for i in 1..10 {
            index_file.append(i * 2, i * 2 * 100)?;
        }

        assert_eq!(index_file.num_entries, 9);
        drop(index_file);

        let open_index_file: IndexFileMut<u64> = open_index_file(&path, 0, 100)?;
        assert_eq!(open_index_file.num_entries, 9);
        assert_eq!(open_index_file.key_lookup(6)?, (6, 600));

        Ok(())
    }
}
