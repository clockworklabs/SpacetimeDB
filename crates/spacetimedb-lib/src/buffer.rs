use std::fmt;
use std::str::Utf8Error;

/// Minimal utility for reading & writing the types we need to internal storage, without relying
/// on third party libraries like bytes::Bytes, etc.
/// Meant to be kept slim and trim for use across both native and wasm.

#[derive(Debug, Clone)]
pub enum DecodeError {
    BufferLength,
    InvalidTag,
    InvalidUtf8,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::BufferLength => f.write_str("data too short"),
            DecodeError::InvalidTag => f.write_str("invalid tag for enum"),
            DecodeError::InvalidUtf8 => f.write_str("invalid utf8"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<Utf8Error> for DecodeError {
    fn from(_: Utf8Error) -> Self {
        DecodeError::InvalidUtf8
    }
}

pub trait BufWriter {
    fn put_slice(&mut self, slice: &[u8]);
    fn put_u8(&mut self, val: u8) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_u16(&mut self, val: u16) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_u32(&mut self, val: u32) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_u64(&mut self, val: u64) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_u128(&mut self, val: u128) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_i8(&mut self, val: i8) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_i16(&mut self, val: i16) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_i32(&mut self, val: i32) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_i64(&mut self, val: i64) {
        self.put_slice(&val.to_le_bytes())
    }
    fn put_i128(&mut self, val: i128) {
        self.put_slice(&val.to_le_bytes())
    }
}

pub trait BufReader {
    fn get_slice(&mut self, size: usize) -> Result<&[u8], DecodeError>;
    fn remaining(&self) -> usize;

    fn get_u8(&mut self) -> Result<u8, DecodeError> {
        self.get_array().map(u8::from_le_bytes)
    }
    fn get_u16(&mut self) -> Result<u16, DecodeError> {
        self.get_array().map(u16::from_le_bytes)
    }
    fn get_u32(&mut self) -> Result<u32, DecodeError> {
        self.get_array().map(u32::from_le_bytes)
    }
    fn get_u64(&mut self) -> Result<u64, DecodeError> {
        self.get_array().map(u64::from_le_bytes)
    }
    fn get_u128(&mut self) -> Result<u128, DecodeError> {
        self.get_array().map(u128::from_le_bytes)
    }
    fn get_i8(&mut self) -> Result<i8, DecodeError> {
        self.get_array().map(i8::from_le_bytes)
    }
    fn get_i16(&mut self) -> Result<i16, DecodeError> {
        self.get_array().map(i16::from_le_bytes)
    }
    fn get_i32(&mut self) -> Result<i32, DecodeError> {
        self.get_array().map(i32::from_le_bytes)
    }
    fn get_i64(&mut self) -> Result<i64, DecodeError> {
        self.get_array().map(i64::from_le_bytes)
    }
    fn get_i128(&mut self) -> Result<i128, DecodeError> {
        self.get_array().map(i128::from_le_bytes)
    }

    fn get_array<const C: usize>(&mut self) -> Result<[u8; C], DecodeError> {
        let mut buf: [u8; C] = [0; C];
        self.get_into_array(&mut buf, C)?;
        Ok(buf)
    }

    fn get_into_array<const C: usize>(&mut self, buf: &mut [u8; C], amount: usize) -> Result<(), DecodeError> {
        let bytes = self.get_slice(amount)?;
        buf.copy_from_slice(bytes);
        Ok(())
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

impl BufReader for &[u8] {
    fn get_slice(&mut self, size: usize) -> Result<&[u8], DecodeError> {
        if self.len() < size {
            return Err(DecodeError::BufferLength);
        }
        let (ret, rest) = self.split_at(size);
        *self = rest;
        Ok(ret)
    }

    fn remaining(&self) -> usize {
        self.len()
    }
}

pub struct Cursor<B> {
    pub buf: B,
    pub pos: usize,
}
impl<B> Cursor<B> {
    pub fn new(buf: B) -> Self {
        Self { buf, pos: 0 }
    }
}
impl<B: AsRef<[u8]>> BufReader for Cursor<B> {
    fn get_slice(&mut self, size: usize) -> Result<&[u8], DecodeError> {
        let ret = self.buf.as_ref()[self.pos..]
            .get(..size)
            .ok_or(DecodeError::BufferLength)?;
        self.pos += size;
        Ok(ret)
    }

    fn remaining(&self) -> usize {
        self.buf.as_ref().len() - self.pos
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
