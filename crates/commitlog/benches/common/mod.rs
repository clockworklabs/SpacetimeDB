use std::io;

use tempfile::{tempdir_in, TempDir};

mod payload;
#[allow(unused)]
pub use payload::{Payload, PayloadDecoder};

/// Create a temporary directory in `$CARGO_TARGET_TMPDIR`.
///
/// `$TMPDIR` often uses a ramdisk, which isn't too useful for benchmarking disk I/O.
pub fn tempdir() -> io::Result<TempDir> {
    tempdir_in(env!("CARGO_TARGET_TMPDIR"))
}
