//! Minimal utility for reading & writing the types we need to internal storage,
//! without relying on types in third party libraries like `bytes::Bytes`, etc.
//! Meant to be kept slim and trim for use across both native and WASM.

use crate::{i256, u256};
use core::cell::Cell;
use core::fmt;
use core::str::Utf8Error;

/// An error that occurred when decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Not enough data was provided in the input.
    BufferLength {
        for_type: &'static str,
        expected: usize,
        given: usize,
    },
    /// Length did not match the statically expected length.
    InvalidLen { expected: usize, given: usize },
    /// The tag does not exist for the sum.
    InvalidTag { tag: u8, sum_name: Option<String> },
    /// Expected data to be UTF-8 but it wasn't.
    InvalidUtf8,
    /// Expected the byte to be 0 or 1 to be a valid bool.
    InvalidBool(u8),
    /// Custom error not in the other variants of `DecodeError`.
    Other(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::BufferLength {
                for_type,
                expected,
                given,
            } => write!(f, "data too short for {for_type}: Expected {expected}, given {given}"),
            DecodeError::InvalidLen { expected, given } => {
                write!(f, "unexpected data length: Expected {expected}, given {given}")
            }
            DecodeError::InvalidTag { tag, sum_name } => {
                write!(
                    f,
                    "unknown tag {tag:#x} for sum type {}",
                    sum_name.as_deref().unwrap_or("<unknown>")
                )
            }
            DecodeError::InvalidUtf8 => f.write_str("invalid utf8"),
            DecodeError::InvalidBool(byte) => write!(f, "byte {byte} not valid as `bool` (must be 0 or 1)"),
            DecodeError::Other(err) => f.write_str(err),
        }
    }
}
impl From<DecodeError> for String {
    fn from(err: DecodeError) -> Self {
        err.to_string()
    }
}
impl std::error::Error for DecodeError {}

impl From<Utf8Error> for DecodeError {
    fn from(_: Utf8Error) -> Self {
        DecodeError::InvalidUtf8
    }
}

/// A buffered writer of some kind.
pub trait BufWriter {
    /// Writes the `slice` to the buffer.
    ///
    /// This is the only method implementations are required to define.
    /// All other methods are provided.
    fn put_slice(&mut self, slice: &[u8]);

    /// Writes a `u8` to the buffer in little-endian (LE) encoding.
    fn put_u8(&mut self, val: u8) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes a `u16` to the buffer in little-endian (LE) encoding.
    fn put_u16(&mut self, val: u16) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes a `u32` to the buffer in little-endian (LE) encoding.
    fn put_u32(&mut self, val: u32) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes a `u64` to the buffer in little-endian (LE) encoding.
    fn put_u64(&mut self, val: u64) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes a `u128` to the buffer in little-endian (LE) encoding.
    fn put_u128(&mut self, val: u128) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes a `u256` to the buffer in little-endian (LE) encoding.
    fn put_u256(&mut self, val: u256) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes an `i8` to the buffer in little-endian (LE) encoding.
    fn put_i8(&mut self, val: i8) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes an `i16` to the buffer in little-endian (LE) encoding.
    fn put_i16(&mut self, val: i16) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes an `i32` to the buffer in little-endian (LE) encoding.
    fn put_i32(&mut self, val: i32) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes an `i64` to the buffer in little-endian (LE) encoding.
    fn put_i64(&mut self, val: i64) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes an `i128` to the buffer in little-endian (LE) encoding.
    fn put_i128(&mut self, val: i128) {
        self.put_slice(&val.to_le_bytes())
    }

    /// Writes an `i256` to the buffer in little-endian (LE) encoding.
    fn put_i256(&mut self, val: i256) {
        self.put_slice(&val.to_le_bytes())
    }
}

macro_rules! get_int {
    ($self:ident, $int:ident) => {
        match $self.get_array_chunk() {
            Some(&arr) => Ok($int::from_le_bytes(arr)),
            None => Err(DecodeError::BufferLength {
                for_type: stringify!($int),
                expected: std::mem::size_of::<$int>(),
                given: $self.remaining(),
            }),
        }
    };
}

/// A buffered reader of some kind.
///
/// The lifetime `'de` allows the output of deserialization to borrow from the input.
pub trait BufReader<'de> {
    /// Reads and returns a chunk of `.len() = size` advancing the cursor iff `self.remaining() >= size`.
    fn get_chunk(&mut self, size: usize) -> Option<&'de [u8]>;

    /// Returns the number of bytes left to read in the input.
    fn remaining(&self) -> usize;

    /// Reads and returns a chunk of `.len() = N` as an array, advancing the cursor.
    #[inline]
    fn get_array_chunk<const N: usize>(&mut self) -> Option<&'de [u8; N]> {
        self.get_chunk(N)?.try_into().ok()
    }

    /// Reads and returns a byte slice of `.len() = size` advancing the cursor.
    #[inline]
    fn get_slice(&mut self, size: usize) -> Result<&'de [u8], DecodeError> {
        self.get_chunk(size).ok_or_else(|| DecodeError::BufferLength {
            for_type: "[u8]",
            expected: size,
            given: self.remaining(),
        })
    }

    /// Reads an array of type `[u8; N]` from the input.
    #[inline]
    fn get_array<const N: usize>(&mut self) -> Result<&'de [u8; N], DecodeError> {
        self.get_array_chunk().ok_or_else(|| DecodeError::BufferLength {
            for_type: "[u8; _]",
            expected: N,
            given: self.remaining(),
        })
    }

    /// Reads a `u8` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_u8(&mut self) -> Result<u8, DecodeError> {
        get_int!(self, u8)
    }

    /// Reads a `u16` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_u16(&mut self) -> Result<u16, DecodeError> {
        get_int!(self, u16)
    }

    /// Reads a `u32` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_u32(&mut self) -> Result<u32, DecodeError> {
        get_int!(self, u32)
    }

    /// Reads a `u64` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_u64(&mut self) -> Result<u64, DecodeError> {
        get_int!(self, u64)
    }

    /// Reads a `u128` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_u128(&mut self) -> Result<u128, DecodeError> {
        get_int!(self, u128)
    }

    /// Reads a `u256` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_u256(&mut self) -> Result<u256, DecodeError> {
        get_int!(self, u256)
    }

    /// Reads an `i8` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_i8(&mut self) -> Result<i8, DecodeError> {
        get_int!(self, i8)
    }

    /// Reads an `i16` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_i16(&mut self) -> Result<i16, DecodeError> {
        get_int!(self, i16)
    }

    /// Reads an `i32` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_i32(&mut self) -> Result<i32, DecodeError> {
        get_int!(self, i32)
    }

    /// Reads an `i64` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_i64(&mut self) -> Result<i64, DecodeError> {
        get_int!(self, i64)
    }

    /// Reads an `i128` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_i128(&mut self) -> Result<i128, DecodeError> {
        get_int!(self, i128)
    }

    /// Reads an `i256` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_chunk`](BufReader::get_chunk)'s definition.
    #[inline]
    fn get_i256(&mut self) -> Result<i256, DecodeError> {
        get_int!(self, i256)
    }
}

impl BufWriter for Vec<u8> {
    fn put_slice(&mut self, slice: &[u8]) {
        self.extend_from_slice(slice);
    }
}

impl BufWriter for &mut [u8] {
    fn put_slice(&mut self, slice: &[u8]) {
        if self.len() < slice.len() {
            panic!("not enough buffer space")
        }
        let (buf, rest) = std::mem::take(self).split_at_mut(slice.len());
        buf.copy_from_slice(slice);
        *self = rest;
    }
}

/// A [`BufWriter`] that only counts the bytes.
#[derive(Default)]
pub struct CountWriter {
    /// The number of bytes counted thus far.
    num_bytes: usize,
}

impl CountWriter {
    /// Consumes the counter and returns the final count.
    pub fn finish(self) -> usize {
        self.num_bytes
    }
}

impl BufWriter for CountWriter {
    fn put_slice(&mut self, slice: &[u8]) {
        self.num_bytes += slice.len();
    }
}

/// A [`BufWriter`] that writes the bytes to two writers `W1` and `W2`.
pub struct TeeWriter<W1, W2> {
    pub w1: W1,
    pub w2: W2,
}

impl<W1: BufWriter, W2: BufWriter> TeeWriter<W1, W2> {
    pub fn new(w1: W1, w2: W2) -> Self {
        Self { w1, w2 }
    }
}

impl<W1: BufWriter, W2: BufWriter> BufWriter for TeeWriter<W1, W2> {
    fn put_slice(&mut self, slice: &[u8]) {
        self.w1.put_slice(slice);
        self.w2.put_slice(slice);
    }
}

impl<'de> BufReader<'de> for &'de [u8] {
    #[inline]
    fn get_chunk(&mut self, size: usize) -> Option<&'de [u8]> {
        let (ret, rest) = self.split_at_checked(size)?;
        *self = rest;
        Some(ret)
    }

    #[inline]
    fn get_array_chunk<const N: usize>(&mut self) -> Option<&'de [u8; N]> {
        let (ret, rest) = self.split_first_chunk()?;
        *self = rest;
        Some(ret)
    }

    #[inline(always)]
    fn remaining(&self) -> usize {
        self.len()
    }
}

/// A cursor based [`BufReader<'de>`] implementation.
#[derive(Debug)]
pub struct Cursor<I> {
    /// The underlying input read from.
    pub buf: I,
    /// The position within the reader.
    pub pos: Cell<usize>,
}

impl<I> Cursor<I> {
    /// Returns a new cursor on the provided `buf` input.
    ///
    /// The cursor starts at the beginning of `buf`.
    pub fn new(buf: I) -> Self {
        Self { buf, pos: 0.into() }
    }
}

impl<'de, I: AsRef<[u8]>> BufReader<'de> for &'de Cursor<I> {
    #[inline]
    fn get_chunk(&mut self, size: usize) -> Option<&'de [u8]> {
        // "Read" the slice `buf[pos..size]`.
        let buf = &self.buf.as_ref()[self.pos.get()..];
        let ret = buf.get(..size)?;

        // Advance the cursor by `size` bytes.
        self.pos.set(self.pos.get() + size);

        Some(ret)
    }

    #[inline]
    fn get_array_chunk<const N: usize>(&mut self) -> Option<&'de [u8; N]> {
        // "Read" the slice `buf[pos..size]`.
        let buf = &self.buf.as_ref()[self.pos.get()..];
        let ret = buf.first_chunk()?;

        // Advance the cursor by `size` bytes.
        self.pos.set(self.pos.get() + N);

        Some(ret)
    }

    fn remaining(&self) -> usize {
        self.buf.as_ref().len() - self.pos.get()
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::{BufReader, BufWriter};

    #[test]
    fn test_simple_encode_decode() {
        let mut writer: Vec<u8> = vec![];
        writer.put_u8(5);
        writer.put_u32(6);
        writer.put_u64(7);

        let arr_val = [1, 2, 3, 4];
        writer.put_slice(&arr_val[..]);

        let mut reader = writer.as_slice();
        assert_eq!(reader.get_u8().unwrap(), 5);
        assert_eq!(reader.get_u32().unwrap(), 6);
        assert_eq!(reader.get_u64().unwrap(), 7);

        let slice = reader.get_slice(4).unwrap();
        assert_eq!(slice, arr_val);

        // reading beyond buffer end should fail
        assert!(reader.get_slice(1).is_err());
        assert!(reader.get_slice(123).is_err());
        assert!(reader.get_u64().is_err());
        assert!(reader.get_u32().is_err());
        assert!(reader.get_u8().is_err());
    }
}
