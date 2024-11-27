//! Shallow byte-partitied directory tries.
//!
//! This is an implementation of the same on-disk data structure as
//! [the git object store](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects),
//! used for storing objects in an object store keyed on a content-addressed 32-byte hash.
//!
//! Objects' location in the trie is computed based on the (64 character) hexadecimal encoding of their (32 byte) Blake3 hash.
//! The leading 2 digits of this hash are the name of the directory,
//! and the remaining 62 digits are the name of the file itself.
//!
//! Storing files in this way is beneficial because directories internally are unsorted arrays of file entries,
//! so searching for a file within a directory is O(directory_size).
//! The trie structure implemented here is still O(n), but with a drastically reduced constant factor (1/128th),
//! which we expect to shrink the linear lookups to an acceptable size.

use crate::compression::{CompressReader, CompressType, CompressWriter};
use std::{
    fs::{create_dir_all, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

/// [`OpenOptions`] corresponding to opening a file with `O_EXCL`,
/// i.e. creating a new writeable file, failing if it already exists.
pub fn o_excl() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    options
}

/// [`OpenOptions`] corresponding to opening a file with `O_RDONLY`,
/// i.e. opening an existing file for reading.
pub fn o_rdonly() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true);
    options
}

/// Counter for objects written newly to disk versus hardlinked,
/// for diagnostic purposes with operations that may hardlink or write.
///
/// See [`DirTrie::hardlink_or_write`].
#[derive(Default, Debug)]
pub struct CountCreated {
    pub objects_written: u64,
    pub objects_hardlinked: u64,
}

/// A directory trie.
pub struct DirTrie {
    /// The directory name at which the dir trie is stored.
    root: PathBuf,
    /// The [CompressType] used for files in this trie.
    compress_type: CompressType,
}

const FILE_ID_BYTES: usize = 32;
const FILE_ID_HEX_CHARS: usize = FILE_ID_BYTES * 2;

type FileId = [u8; FILE_ID_BYTES];

/// The number of leading hex chars taken from a `FileId` as the subdirectory name.
const DIR_HEX_CHARS: usize = 2;

impl DirTrie {
    /// Returns the root of this `DirTrie` on disk.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Open the directory trie at `root`,
    /// creating the root directory if it doesn't exist.
    ///
    /// Returns an error if the `root` cannot be created as a directory.
    /// See documentation on [`create_dir_all`] for more details.
    pub fn open(root: PathBuf, compress_type: CompressType) -> Result<Self, io::Error> {
        create_dir_all(&root)?;
        Ok(Self { root, compress_type })
    }

    fn file_path(&self, file_id: &FileId) -> PathBuf {
        // TODO(perf, bikeshedding): avoid allocating a `String`.
        let file_id_hex = hex::encode(file_id);

        let mut file_path = self.root.clone();
        // Two additional chars for slashes.
        file_path.reserve(FILE_ID_HEX_CHARS + 2);

        // The path will look like `root/xx/yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy`,
        // where `xx` are the leading hex characters of the file id,
        // and the `y`s are the remaining 62 hex characters.
        file_path.push(&file_id_hex[..DIR_HEX_CHARS]);
        file_path.push(&file_id_hex[DIR_HEX_CHARS..]);

        file_path
    }

    /// Hardlink the entry for `file_id` from `src_repo` into `self`.
    ///
    /// Hardlinking makes the object shared between both [`DirTrie`]s
    /// without copying the data on-disk.
    /// Note that this is only possible within a single file system;
    /// this method will likely return an error if `self` and `src_repo`
    /// are in different file systems.
    /// See [Wikipedia](https://en.wikipedia.org/wiki/Hard_link) for more information on hard links.
    ///
    /// Returns `Ok(true)` if the `file_id` existed in `src_repo` and was successfully linked into `self`,
    /// `Ok(false)` if the `file_id` did not exist in `src_repo`,
    /// or an `Err` if a filesystem operation failed.
    ///
    /// The object's hash is not verified against its `file_id`,
    /// so if `file_id` is corrupted within `src_repo`,
    /// the corrupted object will be hardlinked into `self`.
    pub fn try_hardlink_from(&self, src_repo: &DirTrie, file_id: &FileId) -> Result<bool, io::Error> {
        let src_file = src_repo.file_path(file_id);
        if src_file.is_file() {
            let dst_file = self.file_path(file_id);
            Self::create_parent(&dst_file)?;
            std::fs::hard_link(src_file, dst_file)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// True if `file_id` is the key for an object present in this trie.
    ///
    /// Internally calls [`Path::is_file`]; see documentation on that method for detailed semantics.
    pub fn contains_entry(&self, file_id: &FileId) -> bool {
        let path = self.file_path(file_id);

        path.is_file()
    }

    fn create_parent(file: &Path) -> Result<(), io::Error> {
        // Known to succeed because `self.file_path` creates a path with a parent.
        let dir = file.parent().unwrap();

        create_dir_all(dir)?;
        Ok(())
    }

    /// Hardlink `file_id` from `src_repo` into `self`, or create it in `self` containing `contents`.
    ///
    /// `contents` is a thunk which will be called only if the `src_repo` does not contain `file_id`
    /// in order to compute the file contents.
    /// This allows callers to avoid expensive serialization if the object already exists in `src_repo`.
    ///
    /// If the source file exists but the hardlink operation fails, this method returns an error.
    /// In this case, the destination file is not created.
    /// See [`Self::try_hardlink_from`].
    pub fn hardlink_or_write<Bytes: AsRef<[u8]>>(
        &self,
        src_repo: Option<&DirTrie>,
        file_id: &FileId,
        contents: impl FnOnce() -> Bytes,
        counter: &mut CountCreated,
    ) -> Result<(), io::Error> {
        if self.contains_entry(file_id) {
            return Ok(());
        }

        if let Some(src_repo) = src_repo {
            if self.try_hardlink_from(src_repo, file_id)? {
                counter.objects_hardlinked += 1;
                return Ok(());
            }
        }

        let mut file = self.open_entry_writer(file_id, self.compress_type)?;
        let contents = contents();
        file.write_all(contents.as_ref())?;
        counter.objects_written += 1;
        Ok(())
    }

    /// Open the file keyed with `file_id` for reading.
    ///
    /// It will be decompressed based on the file's magic bytes.
    ///
    /// It will be opened with [`o_rdonly`].
    pub fn open_entry_reader(&self, file_id: &FileId) -> Result<CompressReader, io::Error> {
        let path = self.file_path(file_id);
        Self::create_parent(&path)?;
        CompressReader::new(o_rdonly().open(path)?)
    }

    /// Open the file keyed with `file_id` for writing.
    ///
    /// If `ty` is [`CompressType::None`], the file will be written uncompressed.
    ///
    /// The file will be opened with [`o_excl`].
    pub fn open_entry_writer(
        &self,
        file_id: &FileId,
        compress_type: CompressType,
    ) -> Result<CompressWriter, io::Error> {
        let path = self.file_path(file_id);
        Self::create_parent(&path)?;
        CompressWriter::new(o_excl().open(path)?, compress_type)
    }

    /// Open the entry keyed with `file_id` and read it into a `Vec<u8>`.
    pub fn read_entry(&self, file_id: &FileId) -> Result<Vec<u8>, io::Error> {
        let mut file = self.open_entry_reader(file_id)?;
        let mut buf = Vec::with_capacity(file.metadata()?.len() as usize);
        // TODO(perf): Async IO?
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Read;

    const TEST_ID: FileId = [0xa5; FILE_ID_BYTES];
    const TEST_STRING: &[u8] = b"test string";

    fn with_test_dir_trie(f: impl FnOnce(DirTrie)) {
        let root = tempdir::TempDir::new("test_dir_trie").unwrap();
        let trie = DirTrie::open(root.path().to_path_buf(), CompressType::None).unwrap();
        f(trie)
    }

    /// Write the [`TEST_STRING`] into the entry [`TEST_ID`].
    fn write_test_string(trie: &DirTrie) {
        let mut file = trie.open_entry_writer(&TEST_ID, CompressType::None).unwrap();
        file.write_all(TEST_STRING).unwrap();
    }

    /// Read the entry [`TEST_ID`] and assert that its contents match the [`TEST_STRING`].
    fn read_test_string(trie: &DirTrie) {
        let mut file = trie.open_entry_reader(&TEST_ID).unwrap();
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();
        assert_eq!(&contents, TEST_STRING);
    }

    #[test]
    fn create_retrieve() {
        with_test_dir_trie(|trie| {
            // The trie starts empty, so it doesn't contain the `TEST_ID`'s file.
            assert!(!trie.contains_entry(&TEST_ID));

            // Create an entry in the trie and write some data to it.
            write_test_string(&trie);

            // The trie now has that entry.
            assert!(trie.contains_entry(&TEST_ID));

            // Open the entry and read its data back.
            read_test_string(&trie);
        })
    }

    #[test]
    fn hardlink() {
        with_test_dir_trie(|src| {
            with_test_dir_trie(|dst| {
                // Both tries starts empty, so they don't contain the `TEST_ID`'s file.
                assert!(!src.contains_entry(&TEST_ID));
                assert!(!dst.contains_entry(&TEST_ID));

                // Create an entry in `src` and write some data to it.
                write_test_string(&src);

                // The `src` now contains the entry, but the `dst` still doesn't.
                assert!(src.contains_entry(&TEST_ID));
                assert!(!dst.contains_entry(&TEST_ID));

                // Hardlink the entry from `src` into `dst`.
                assert!(dst.try_hardlink_from(&src, &TEST_ID).unwrap());

                // After hardlinking, the file is now in `dst`.
                assert!(dst.contains_entry(&TEST_ID));
                // Open the entry in `dst` and read its data back.
                read_test_string(&dst);

                // The file is still also in `src`, and its data hasn't changed.
                assert!(src.contains_entry(&TEST_ID));
                read_test_string(&src);
            })
        })
    }

    #[test]
    fn open_options() {
        with_test_dir_trie(|trie| {
            // The trie starts empty, so it doesn't contain the `TEST_ID`'s file.
            assert!(!trie.contains_entry(&TEST_ID));

            // Because the file isn't there, we can't open it.
            assert!(trie.open_entry_reader(&TEST_ID).is_err());

            // Create an entry in the trie and write some data to it.
            write_test_string(&trie);

            // The trie now has that entry.
            assert!(trie.contains_entry(&TEST_ID));

            // Because the file is there, we can't create it.
            assert!(trie.open_entry_writer(&TEST_ID, CompressType::Zstd).is_err());
        })
    }
}
