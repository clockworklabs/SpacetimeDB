use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use zstd_framed;
use zstd_framed::{ZstdReader, ZstdWriter};

const ZSTD_MAGIC_BYTES: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

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
pub enum CompressReader<'a> {
    None(BufReader<File>),
    Zstd(ZstdReader<'a, BufReader<File>>),
}

impl CompressReader<'_> {
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
                ZSTD_MAGIC_BYTES => CompressReader::Zstd(ZstdReader::builder(inner).build()?),
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

impl Read for CompressReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            CompressReader::None(inner) => inner.read(buf),
            CompressReader::Zstd(inner) => inner.read(buf),
        }
    }
}

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
