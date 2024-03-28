use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::{
    fs::{self, File},
    path::PathBuf,
};

use log::debug;

use super::Repo;

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

/// A commitlog repository [`Repo`] which stores commits in ordinary files on
/// disk.
#[derive(Clone, Debug)]
pub struct Fs {
    /// The base directory within which segment files will be stored.
    pub root: PathBuf,
    /// Use `O_DIRECT` or platform equivalent.
    ///
    /// Setting this to true will make reads and writes bypass the OS's page
    /// cache and access the storage devices directly.
    ///
    /// Default: true
    pub direct_io: bool,
    /// Use `O_DSYNC` or plaform equivalent.
    ///
    /// Setting this to true will make writes behave as if followed by a
    /// call to `fdatasync(2)`.
    ///
    /// Note that this has a performance impact, and that `fsync(2)` may still
    /// be required to guarantee durability.
    ///
    /// Has no effect on macOS.
    ///
    /// Default: false
    pub sync_io: bool,
}

impl Fs {
    /// Create a commitlog repository which stores segments in the directory `root`.
    ///
    /// `root` must name an extant, accessible, writeable directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            direct_io: true,
            sync_io: false,
        }
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
        }

        Ok(sz)
    }
}

impl Repo for Fs {
    type Segment = File;

    fn create_segment(&self, offset: u64) -> io::Result<Self::Segment> {
        open(
            self.segment_path(offset),
            File::options().read(true).write(true).create_new(true),
            self.direct_io,
            self.sync_io,
        )
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
        open(
            self.segment_path(offset),
            File::options().read(true).write(true),
            self.direct_io,
            self.sync_io,
        )
    }

    fn remove_segment(&self, offset: u64) -> io::Result<()> {
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
}

pub fn open(path: impl AsRef<Path>, opts: &mut OpenOptions, direct_io: bool, sync_io: bool) -> io::Result<File> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut flags = 0;
        if direct_io {
            flags |= libc::O_DIRECT;
        }
        if sync_io {
            flags |= libc::O_DSYNC;
        }
        opts.custom_flags(flags);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use winapi::um::winbase::*;

        // For reference:
        //
        // FILE_FLAG_NO_BUFFERING: https://docs.microsoft.com/en-us/windows/win32/fileio/file-buffering
        // FILE_FLAG_WRITE_THROUGH: https://docs.microsoft.com/en-us/windows/win32/fileio/file-caching
        let mut flags = 0;
        if direct_io {
            flags |= FILE_FLAG_NO_BUFFERING;
        }
        if sync_io {
            flags |= FILE_FLAG_WRITE_THROUGH;
        }
        opts.custom_flags(flags);
    }

    let file = opts.open(path)?;

    #[cfg(target_os = "macos")]
    {
        // On macOS, O_DIRECT may or may not be defined, its functional
        // equivalent is the F_NOCACHE fcntl. It may be necessary to set
        // F_PREALLOCATE as well[1].
        //
        // O_DSYNC is considered non-functional[2]. It is not clear if
        // an equivalent exists, the F_FULLFSYNC fcntl seems to be a
        // oneshot equivalent to just calling `fsync(2)`.
        //
        // [1]: https://forums.developer.apple.com/forums/thread/25464
        // [2]: https://x.com/jorandirkgreef/status/1532314169604726784

        use libc::{fcntl, F_NOCACHE};
        use std::os::fd::AsRawFd;

        let fd = file.as_raw_fd();
        if direct_io {
            let ret = unsafe { fcntl(fd, F_NOCACHE, 1) };
            if ret != 0 {
                return Err(io::Error::from_raw_os_error(ret));
            }
        }
    }

    Ok(file)
}
