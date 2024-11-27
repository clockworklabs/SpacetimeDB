use lz4_flex::frame::{AutoFinishEncoder as Lz4Encoder, FrameDecoder, FrameEncoder};
use snap::read::FrameDecoder as SnapDecoder;
use snap::write::FrameEncoder as SnapEncoder;
use std::fs::{File, Metadata};
use std::io;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use zstd::stream::AutoFinishEncoder;
use zstd::{Decoder, Encoder};

const ZSTD_MAGIC_BYTES: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
const LZ4_MAGIC_BYTES: [u8; 4] = [0x04, 0x22, 0x4D, 0x18];
const SNAP_MAGIC_BYTES: [u8; 4] = [0xFF, 0x06, 0x00, 0x00];

/// Compression type
///
/// if `None`, the file is not compressed, otherwise it will be compressed using the specified algorithm.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CompressType {
    None,
    Zstd,
    Lz4,
    Snap,
}

/// A reader that can read compressed files
pub enum CompressReader<'a> {
    None(BufReader<File>),
    Zstd(Decoder<'a, BufReader<File>>),
    Lz4(FrameDecoder<BufReader<File>>),
    Snap(SnapDecoder<BufReader<File>>),
}

impl<'a> CompressReader<'a> {
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
                LZ4_MAGIC_BYTES => {
                    let decoder = FrameDecoder::new(BufReader::new(inner));
                    CompressReader::Lz4(decoder)
                }
                SNAP_MAGIC_BYTES => {
                    let decoder = SnapDecoder::new(BufReader::new(inner));
                    CompressReader::Snap(decoder)
                }
                _ => CompressReader::None(BufReader::new(inner)),
            }
        } else {
            CompressReader::None(BufReader::new(inner))
        })
    }

    pub fn metadata(&self) -> io::Result<Metadata> {
        match self {
            CompressReader::None(inner) => inner.get_ref().metadata(),
            CompressReader::Zstd(inner) => inner.get_ref().get_ref().metadata(),
            CompressReader::Lz4(inner) => inner.get_ref().get_ref().metadata(),
            CompressReader::Snap(inner) => inner.get_ref().get_ref().metadata(),
        }
    }
}

impl<'a> Read for CompressReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            CompressReader::None(inner) => inner.read(buf),
            CompressReader::Zstd(inner) => inner.read(buf),
            CompressReader::Lz4(inner) => inner.read(buf),
            CompressReader::Snap(inner) => inner.read(buf),
        }
    }
}

/// A writer that can write compressed files
pub enum CompressWriter<'a> {
    None(BufWriter<File>),
    Zstd(AutoFinishEncoder<'a, BufWriter<File>>),
    Lz4(Lz4Encoder<BufWriter<File>>),
    Snap(Box<SnapEncoder<File>>),
}

impl<'a> CompressWriter<'a> {
    pub fn new(inner: File, compress_type: CompressType) -> io::Result<Self> {
        match compress_type {
            CompressType::None => Ok(CompressWriter::None(BufWriter::new(inner))),
            CompressType::Zstd => Ok(CompressWriter::Zstd(
                Encoder::new(BufWriter::new(inner), 0)?.auto_finish(),
            )),
            CompressType::Lz4 => Ok(CompressWriter::Lz4(
                FrameEncoder::new(BufWriter::new(inner)).auto_finish(),
            )),
            // SnapEncoder does buffer internally, so we don't need to wrap it in a BufWriter
            CompressType::Snap => Ok(CompressWriter::Snap(Box::new(SnapEncoder::new(inner)))),
        }
    }
}

impl<'a> Write for CompressWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            CompressWriter::None(inner) => inner.write(buf),
            CompressWriter::Zstd(inner) => inner.write(buf),
            CompressWriter::Lz4(inner) => inner.write(buf),
            CompressWriter::Snap(inner) => inner.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            CompressWriter::None(inner) => inner.flush(),
            CompressWriter::Zstd(inner) => inner.flush(),
            CompressWriter::Lz4(inner) => inner.flush(),
            CompressWriter::Snap(inner) => inner.flush(),
        }
    }
}
