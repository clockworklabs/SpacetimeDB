//! Minimal utility for reading & writing the types we need to internal storage,
//! without relying on types in third party libraries like `bytes::Bytes`, etc.
//! Meant to be kept slim and trim for use across both native and WASM.

use std::cell::Cell;
use std::fmt;
use std::str::Utf8Error;

/// An error that occurred when decoding.
#[derive(Debug, Clone)]
pub enum DecodeError {
    /// Not enough data was provided in the input.
    BufferLength {
        for_type: String,
        expected: usize,
        given: usize,
    },
    /// The tag does not exist for the sum.
    InvalidTag,
    /// Expected data to be UTF-8 but it wasn't.
    InvalidUtf8,
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
            DecodeError::InvalidTag => f.write_str("invalid tag for sum"),
            DecodeError::InvalidUtf8 => f.write_str("invalid utf8"),
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
}

/// A buffered reader of some kind.
///
/// The lifetime `'de` allows the output of deserialization to borrow from the input.
pub trait BufReader<'de> {
    /// Reads and returns a byte slice of `.len() = size` advancing the cursor.
    fn get_slice(&mut self, size: usize) -> Result<&'de [u8], DecodeError>;

    /// Returns the number of bytes left to read in the input.
    fn remaining(&self) -> usize;

    /// Reads a `u8` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_u8(&mut self) -> Result<u8, DecodeError> {
        self.get_array().map(u8::from_le_bytes)
    }

    /// Reads a `u16` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_u16(&mut self) -> Result<u16, DecodeError> {
        self.get_array().map(u16::from_le_bytes)
    }

    /// Reads a `u32` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_u32(&mut self) -> Result<u32, DecodeError> {
        self.get_array().map(u32::from_le_bytes)
    }

    /// Reads a `u64` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_u64(&mut self) -> Result<u64, DecodeError> {
        self.get_array().map(u64::from_le_bytes)
    }

    /// Reads a `u128` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_u128(&mut self) -> Result<u128, DecodeError> {
        self.get_array().map(u128::from_le_bytes)
    }

    /// Reads an `i8` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_i8(&mut self) -> Result<i8, DecodeError> {
        self.get_array().map(i8::from_le_bytes)
    }

    /// Reads an `i16` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_i16(&mut self) -> Result<i16, DecodeError> {
        self.get_array().map(i16::from_le_bytes)
    }

    /// Reads an `i32` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_i32(&mut self) -> Result<i32, DecodeError> {
        self.get_array().map(i32::from_le_bytes)
    }

    /// Reads an `i64` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_i64(&mut self) -> Result<i64, DecodeError> {
        self.get_array().map(i64::from_le_bytes)
    }

    /// Reads an `i128` in little endian (LE) encoding from the input.
    ///
    /// This method is provided for convenience
    /// and is derived from [`get_slice`](BufReader::get_slice)'s definition.
    fn get_i128(&mut self) -> Result<i128, DecodeError> {
        self.get_array().map(i128::from_le_bytes)
    }

    /// Reads an array of type `[u8; C]` from the input.
    fn get_array<const C: usize>(&mut self) -> Result<[u8; C], DecodeError> {
        let mut buf: [u8; C] = [0; C];
        buf.copy_from_slice(self.get_slice(C)?);
        Ok(buf)
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

impl<'de> BufReader<'de> for &'de [u8] {
    fn get_slice(&mut self, size: usize) -> Result<&'de [u8], DecodeError> {
        if self.len() < size {
            return Err(DecodeError::BufferLength {
                for_type: "[u8]".into(),
                expected: size,
                given: self.len(),
            });
        }
        let (ret, rest) = self.split_at(size);
        *self = rest;
        Ok(ret)
    }

    fn remaining(&self) -> usize {
        self.len()
    }
}

/// A cursor based [`BufReader<'de>`] implementation.
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
    fn get_slice(&mut self, size: usize) -> Result<&'de [u8], DecodeError> {
        // "Read" the slice `buf[pos..size]`.
        let ret = self.buf.as_ref()[self.pos.get()..]
            .get(..size)
            .ok_or(DecodeError::BufferLength {
                for_type: "Cursor".into(),
                expected: (self.pos.get()..size).len(),
                given: size,
            })?;

        // Advance the cursor by `size` bytes.
        self.pos.set(self.pos.get() + size);

        Ok(ret)
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
