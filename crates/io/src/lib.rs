//! Narrow facade for SpacetimeDB-owned async IO boundaries.
//!
//! This crate currently re-exports the Tokio filesystem, IO, and network APIs
//! that SpacetimeDB code is allowed to depend on directly.
//!
//! This crate is intentionally small. It is a migration point for filesystem and
//! network APIs reached by deterministic simulation tests, not a general runtime
//! abstraction for tasks, clocks, blocking work, or shutdown.

pub mod fs {
    pub use tokio::fs::*;

    #[cfg(simulation)]
    use std::{
        io::{self, Read as _},
        pin::Pin,
        task::{Context, Poll},
    };

    /// Async reader type returned by [`file_from_std`].
    #[cfg(not(simulation))]
    pub type FileFromStd = tokio::fs::File;

    /// Async reader type returned by [`file_from_std`].
    #[cfg(simulation)]
    pub type FileFromStd = StdFileAsyncReader;

    /// Convert a standard file handle into an async reader.
    ///
    /// Tokio supports this directly. The simulated filesystem type does not
    /// wrap existing OS files, so simulation builds use a small `AsyncRead`
    /// adapter for call sites that only need to stream an already-opened std
    /// file.
    #[cfg(not(simulation))]
    pub fn file_from_std(file: std::fs::File) -> FileFromStd {
        tokio::fs::File::from_std(file)
    }

    /// Convert a standard file handle into an async reader.
    #[cfg(simulation)]
    pub fn file_from_std(file: std::fs::File) -> FileFromStd {
        StdFileAsyncReader(file)
    }

    /// Async-read adapter for standard files in simulation builds.
    #[cfg(simulation)]
    pub struct StdFileAsyncReader(std::fs::File);

    #[cfg(simulation)]
    impl tokio::io::AsyncRead for StdFileAsyncReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            match self.0.read(buf.initialize_unfilled()) {
                Ok(n) => {
                    buf.advance(n);
                    Poll::Ready(Ok(()))
                }
                Err(e) => Poll::Ready(Err(e)),
            }
        }
    }
}

pub mod io {
    pub use tokio::io::*;
}

pub mod net {
    pub use tokio::net::*;
}
