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

use std::{
    fs::{create_dir_all, File, OpenOptions, ReadDir},
    io,
    path::PathBuf,
};

/// A directory trie.
pub struct DirTrie {
    /// The directory name at which the dir trie is stored.
    root: PathBuf,
}

const FILE_ID_BYTES: usize = blake3::OUT_LEN;
const FILE_ID_HEX_CHARS: usize = FILE_ID_BYTES * 2;

type FileId = [u8; FILE_ID_BYTES];

/// The number of leading hex chars taken from a `FileId` as the subdirectory name.
const DIR_HEX_CHARS: usize = 2;

impl DirTrie {
    pub fn open(root: PathBuf) -> Result<Self, io::Error> {
        create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn file_path(&self, file_id: &FileId) -> PathBuf {
        // TODO(perf, bikeshedding): avoid allocating a `String`.
        let file_id_hex = hex::encode(file_id);

        let mut file_path = self.root.clone();
        // Two additional chars for slashes.
        file_path.reserve(FILE_ID_HEX_CHARS + 2);

        file_path.push(&file_id_hex[..DIR_HEX_CHARS]);
        file_path.push(&file_id_hex[DIR_HEX_CHARS..]);

        file_path
    }

    #[allow(unused)]
    pub fn contains_entry(&self, file_id: &FileId) -> bool {
        let path = self.file_path(file_id);

        path.is_file()
    }

    pub fn open_entry(&self, file_id: &FileId, options: &OpenOptions) -> Result<File, io::Error> {
        let path = self.file_path(file_id);

        // Known to succeed because `self.file_path` creates a path with a parent.
        let dir = path.parent().unwrap();

        create_dir_all(dir)?;

        options.open(path)
    }

    #[allow(unused)]
    pub fn iter_entries(&self) -> Result<impl Iterator<Item = Result<PathBuf, io::Error>>, io::Error> {
        let subdirs = self.root.read_dir()?;
        Ok(Iter {
            subdirs,
            current_dir: None,
        })
    }
}

pub struct Iter {
    subdirs: ReadDir,

    current_dir: Option<ReadDir>,
}

impl Iterator for Iter {
    type Item = Result<PathBuf, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current_dir) = self.current_dir.as_mut() {
                match current_dir.next() {
                    None => {
                        self.current_dir = None;
                    }
                    Some(Err(e)) => return Some(Err(e)),
                    Some(Ok(dirent)) => return Some(Ok(dirent.path())),
                }
            } else {
                match self.subdirs.next() {
                    None => {
                        return None;
                    }
                    Some(Err(e)) => {
                        return Some(Err(e));
                    }
                    Some(Ok(new_dir)) => match new_dir.path().read_dir() {
                        Err(e) => return Some(Err(e)),
                        Ok(new_dir) => self.current_dir = Some(new_dir),
                    },
                }
            }
        }
    }
}
