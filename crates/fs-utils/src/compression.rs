use std::fs::{File, Metadata};
use std::io;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use zstd::stream::AutoFinishEncoder;
use zstd::{Decoder, Encoder};

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
    Zstd(Decoder<'a, BufReader<File>>),
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
                ZSTD_MAGIC_BYTES => {
                    let decoder = Decoder::new(inner)?;
                    CompressReader::Zstd(decoder)
                }
                _ => CompressReader::None(BufReader::new(inner)),
            }
        } else {
            CompressReader::None(BufReader::new(inner))
        })
    }

    pub fn metadata(&self) -> io::Result<Metadata> {
        match self {
            Self::None(inner) => inner.get_ref().metadata(),
            Self::Zstd(inner) => inner.get_ref().get_ref().metadata(),
        }
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
    Zstd(AutoFinishEncoder<'a, BufWriter<File>>),
}

impl CompressWriter<'_> {
    pub fn new(inner: File, compress_type: CompressType) -> io::Result<Self> {
        match compress_type {
            CompressType::None => Ok(CompressWriter::None(BufWriter::new(inner))),
            CompressType::Zstd => Ok(CompressWriter::Zstd(
                Encoder::new(BufWriter::new(inner), 0)?.auto_finish(),
            )),
        }
    }

    pub fn metadata(&self) -> io::Result<Metadata> {
        match self {
            Self::None(inner) => inner.get_ref().metadata(),
            Self::Zstd(inner) => inner.get_ref().get_ref().metadata(),
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
