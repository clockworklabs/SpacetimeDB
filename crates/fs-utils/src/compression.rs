use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use zstd_framed;
use zstd_framed::{ZstdReader, ZstdWriter};

pub const ZSTD_MAGIC_BYTES: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Helper struct to keep track of the number of files compressed using each algorithm
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct CompressCount {
    pub none: usize,
    pub zstd: usize,
}

/// Compression type
///
/// if `None`, the file is not compressed, otherwise it will be compressed using the specified algorithm.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CompressType {
    None,
    Zstd,
}

/// A reader that can read compressed files
pub enum CompressReader {
    None(BufReader<File>),
    Zstd(Box<ZstdReader<'static, BufReader<File>>>),
}

impl CompressReader {
    /// Create a new CompressReader from a File
    ///
    /// It will detect the compression type using `magic bytes` and return the appropriate reader.
    ///
    /// **Note**: The reader will be return to the original position after detecting the compression type.
    pub fn new(mut inner: File) -> io::Result<Self> {
        let current_pos = inner.stream_position()?;

        let mut magic_bytes = [0u8; 4];
        let bytes_read = inner.read(&mut magic_bytes)?;

        // Restore the original position
        inner.seek(SeekFrom::Start(current_pos))?;

        // Determine compression type
        Ok(if bytes_read == 4 {
            match magic_bytes {
                ZSTD_MAGIC_BYTES => {
                    let table = zstd_framed::table::read_seek_table(&mut inner)?;
                    let mut builder = ZstdReader::builder(inner);
                    if let Some(table) = table {
                        builder = builder.with_seek_table(table);
                    }
                    CompressReader::Zstd(Box::new(builder.build()?))
                }
                _ => CompressReader::None(BufReader::new(inner)),
            }
        } else {
            CompressReader::None(BufReader::new(inner))
        })
    }

    pub fn file_size(&self) -> io::Result<usize> {
        Ok(match self {
            Self::None(inner) => inner.get_ref().metadata()?.len() as usize,
            //TODO: Can't see how to get the file size from ZstdReader
            Self::Zstd(_inner) => 0,
        })
    }

    pub fn compress_type(&self) -> CompressType {
        match self {
            CompressReader::None(_) => CompressType::None,
            CompressReader::Zstd(_) => CompressType::Zstd,
        }
    }
}

impl Read for CompressReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            CompressReader::None(inner) => inner.read(buf),
            CompressReader::Zstd(inner) => inner.read(buf),
        }
    }
}

impl io::BufRead for CompressReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        match self {
            CompressReader::None(inner) => inner.fill_buf(),
            CompressReader::Zstd(inner) => inner.fill_buf(),
        }
    }

    fn consume(&mut self, amt: usize) {
        match self {
            CompressReader::None(inner) => inner.consume(amt),
            CompressReader::Zstd(inner) => inner.consume(amt),
        }
    }
}

impl Seek for CompressReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            CompressReader::None(inner) => inner.seek(pos),
            CompressReader::Zstd(inner) => inner.seek(pos),
        }
    }
}

pub fn new_zstd_writer<'a, W: io::Write>(inner: W, max_frame_size: u32) -> io::Result<ZstdWriter<'a, W>> {
    ZstdWriter::builder(inner)
        .with_compression_level(0)
        .with_seek_table(max_frame_size)
        .build()
}

pub use async_impls::AsyncCompressReader;

/// A writer that can write compressed files
pub enum CompressWriter<'a> {
    None(BufWriter<File>),
    Zstd(ZstdWriter<'a, BufWriter<File>>),
}

impl CompressWriter<'_> {
    pub fn new(inner: File, compress_type: CompressType) -> io::Result<Self> {
        match compress_type {
            CompressType::None => Ok(CompressWriter::None(BufWriter::new(inner))),
            CompressType::Zstd => Ok(CompressWriter::Zstd(
                ZstdWriter::builder(BufWriter::new(inner))
                    .with_compression_level(0)
                    .build()?,
            )),
        }
    }

    pub fn finish(self) -> io::Result<()> {
        match self {
            CompressWriter::None(mut inner) => inner.flush(),
            CompressWriter::Zstd(mut inner) => inner.shutdown(),
        }
    }
}

impl Write for CompressWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            CompressWriter::None(inner) => inner.write(buf),
            CompressWriter::Zstd(inner) => inner.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            CompressWriter::None(inner) => inner.flush(),
            CompressWriter::Zstd(inner) => inner.flush(),
        }
    }
}

mod async_impls {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{self, AsyncBufRead, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
    use zstd_framed::{AsyncZstdReader, AsyncZstdSeekableReader};

    pub enum AsyncCompressReader<R> {
        None(io::BufReader<R>),
        Zstd(Box<AsyncZstdSeekableReader<'static, io::BufReader<R>>>),
    }

    impl<R: AsyncRead + AsyncSeek + Unpin> AsyncCompressReader<R> {
        /// Create a new AsyncCompressReader from a reader
        ///
        /// It will detect the compression type using `magic bytes` and return the appropriate reader.
        ///
        /// **Note**: The reader will be return to the start after detecting the compression type.
        pub async fn new(mut inner: R) -> io::Result<Self> {
            let mut magic_bytes = [0u8; 4];
            let magic_bytes = match inner.read_exact(&mut magic_bytes).await {
                Ok(_) => Some(magic_bytes),
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => None,
                Err(e) => return Err(e),
            };

            // Restore the original position
            inner.seek(io::SeekFrom::Start(0)).await?;

            // Determine compression type
            Ok(match magic_bytes {
                Some(ZSTD_MAGIC_BYTES) => {
                    let table = zstd_framed::table::tokio::read_seek_table(&mut inner).await?;
                    let mut builder = AsyncZstdReader::builder_tokio(inner);
                    if let Some(table) = table {
                        builder = builder.with_seek_table(table);
                    }
                    AsyncCompressReader::Zstd(Box::new(builder.build()?.seekable()))
                }
                _ => AsyncCompressReader::None(io::BufReader::new(inner)),
            })
        }

        pub fn compress_type(&self) -> CompressType {
            match self {
                AsyncCompressReader::None(_) => CompressType::None,
                AsyncCompressReader::Zstd(_) => CompressType::Zstd,
            }
        }
    }

    macro_rules! forward_reader {
    ($self:ident.$method:ident($($args:expr),*)) => {
        match $self.get_mut() {
            AsyncCompressReader::None(r) => Pin::new(r).$method($($args),*),
            AsyncCompressReader::Zstd(r) => Pin::new(r).$method($($args),*),
        }
    };
}
    impl<R: AsyncRead + AsyncSeek + Unpin> AsyncRead for AsyncCompressReader<R> {
        fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut io::ReadBuf<'_>) -> Poll<io::Result<()>> {
            forward_reader!(self.poll_read(cx, buf))
        }
    }
    impl<R: AsyncRead + AsyncSeek + Unpin> AsyncBufRead for AsyncCompressReader<R> {
        fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
            forward_reader!(self.poll_fill_buf(cx))
        }

        fn consume(self: Pin<&mut Self>, amt: usize) {
            forward_reader!(self.consume(amt))
        }
    }
    impl<R: AsyncRead + AsyncSeek + Unpin> AsyncSeek for AsyncCompressReader<R> {
        fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
            forward_reader!(self.start_seek(position))
        }

        fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<u64>> {
            forward_reader!(self.poll_complete(cx))
        }
    }
}
