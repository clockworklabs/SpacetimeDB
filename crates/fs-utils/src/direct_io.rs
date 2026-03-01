mod page;
mod reader;
mod writer;

use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

pub use self::{page::Page, reader::AlignedBufReader, writer::AlignedBufWriter};

/// Open a [File] according to [OpenOptions], enabling `O_DIRECT` or a platform
/// equivalent.
///
/// On all supported platforms, direct I/O requires alignment of memory buffers
/// and file offsets to the logical block size of the filesystem. Wrap the
/// returned [File] in [AlignedBufReader] or [AlignedBufWriter] respectively to
/// have this being taken care of for you.
///
/// # Platform differences
///
/// * Unix (except macOS):
///
///   The file will be opened with the `O_DIRECT` flag.
///
/// * macOS:
///
///   The `F_NOCACHE` fcntl will be set on the opened file.
///   It may be necessary to set `F_PREALLOCATE` as well[1].
///
/// * Windows:
///
///   The file will be opened with [FILE_FLAG_NO_BUFFERING].
///
/// [1]: https://forums.developer.apple.com/forums/thread/25464
/// [FILE_FLAG_NO_BUFFERING]: https://docs.microsoft.com/en-us/windows/win32/fileio/file-buffering
pub fn open_file(path: impl AsRef<Path>, opts: &mut OpenOptions) -> io::Result<File> {
    open_file_impl(path.as_ref(), opts)
}

/// Open the file at `path` for reading in `O_DIRECT` mode and wrap it in an
/// [AlignedBufReader].
pub fn file_reader(path: impl AsRef<Path>) -> io::Result<AlignedBufReader<File>> {
    open_file(path, OpenOptions::new().read(true)).map(AlignedBufReader::new)
}

/// Open the file at `path` for writing in `O_DIRECT` mode and wrap it in an
/// [AlignedBufWriter].
///
/// The file will be created if it does not exist, and truncated if it does.
pub fn file_writer(path: impl AsRef<Path>) -> io::Result<AlignedBufWriter<File>> {
    open_file(path, OpenOptions::new().create(true).write(true).truncate(true)).map(AlignedBufWriter::new)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_file_impl(path: &Path, opts: &mut OpenOptions) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;

    opts.custom_flags(libc::O_DIRECT);
    opts.open(path)
}

#[cfg(target_os = "macos")]
fn open_file_impl(path: &Path, opts: &mut OpenOptions) -> io::Result<File> {
    use libc::{fcntl, F_NOCACHE};
    use std::os::fd::AsRawFd;

    let file = opts.open(path)?;
    let fd = file.as_raw_fd();
    let ret = unsafe { fcntl(fd, F_NOCACHE, 1) };
    if ret != 0 {
        return Err(io::Error::from_raw_os_error(ret));
    }

    Ok(file)
}

#[cfg(target_os = "windows")]
fn open_file_impl(path: &Path, opts: &mut OpenOptions) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_NO_BUFFERING;

    opts.custom_flags(FILE_FLAG_NO_BUFFERING);
    opts.open(path)
}
