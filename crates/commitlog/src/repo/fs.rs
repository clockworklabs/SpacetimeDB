use std::io;
use std::{
    fs::{self, File},
    path::PathBuf,
};

use log::{debug, warn};

use crate::index::{create_index_file, delete_index_file, offset_index_file_path};

use super::{Repo, TxOffset, TxOffsetIndex};

const SEGMENT_FILE_EXT: &str = ".stdb.log";

/// By convention, the file name of a segment consists of the minimum
/// transaction offset contained in it, left-padded with zeroes to 20 digits,
/// and the file extension `.stdb.log`.
pub fn segment_file_name(offset: u64) -> String {
    format!("{offset:0>20}{SEGMENT_FILE_EXT}")
}

// TODO
//
// - should use advisory locks?
//
// Experiment:
//
// - O_DIRECT | O_DSYNC
// - preallocation of disk space
// - io_uring
//

/// A commitlog repository [`Repo`] which stores commits in ordinary files on
/// disk.
#[derive(Clone, Debug)]
pub struct Fs {
    /// The base directory within which segment files will be stored.
    root: PathBuf,
}

impl Fs {
    /// Create a commitlog repository which stores segments in the directory `root`.
    ///
    /// `root` must name an extant, accessible, writeable directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Get the filename for a segment starting with `offset` within this
    /// repository.
    pub fn segment_path(&self, offset: u64) -> PathBuf {
        self.root.join(segment_file_name(offset))
    }

    /// Determine the size on disk as the sum of the sizes of all segments.
    ///
    /// Note that the actively written-to segment (if any) is included.
    pub fn size_on_disk(&self) -> io::Result<u64> {
        let mut sz = 0;
        for offset in self.existing_offsets()? {
            sz += self.segment_path(offset).metadata()?.len();
            // Add the size of the offset index file if present
            sz += offset_index_file_path(&self.root, offset)
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
        }

        Ok(sz)
    }
}

impl Repo for Fs {
    type Segment = File;

    fn create_segment(&self, offset: u64) -> io::Result<Self::Segment> {
        File::options()
            .read(true)
            .append(true)
            .create_new(true)
            .open(self.segment_path(offset))
            .or_else(|e| {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    debug!("segment {offset} already exists");
                    let file = self.open_segment(offset)?;
                    if file.metadata()?.len() == 0 {
                        debug!("segment {offset} is empty");
                        return Ok(file);
                    }
                }

                Err(e)
            })
    }

    fn open_segment(&self, offset: u64) -> io::Result<Self::Segment> {
        File::options().read(true).append(true).open(self.segment_path(offset))
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
        let _ = self.remove_offset_index(offset).map_err(|e| {
            warn!("failed to remove offset index for segment {offset}, error: {e}");
        });
        fs::remove_file(self.segment_path(offset))
    }

    fn existing_offsets(&self) -> io::Result<Vec<u64>> {
        let mut segments = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let path = entry.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let Some(file_name) = name.strip_suffix(SEGMENT_FILE_EXT) else {
                    continue;
                };
                let Ok(offset) = file_name.parse::<u64>() else {
                    continue;
                };

                segments.push(offset);
            }
        }

        segments.sort_unstable();

        Ok(segments)
    }

    fn get_offset_index(&self, offset: TxOffset, cap: u64) -> io::Result<TxOffsetIndex> {
        create_index_file(&self.root, offset, cap)
    }

    fn remove_offset_index(&self, offset: TxOffset) -> io::Result<()> {
        delete_index_file(&self.root, offset)
    }
}
