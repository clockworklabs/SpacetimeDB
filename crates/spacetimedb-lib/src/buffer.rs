/// Minimal utility for reading & writing the types we need to internal storage, without relying
/// on third party libraries like bytes::Bytes, etc.
/// Meant to be kept slim and trim for use across both native and wasm.

#[derive(Debug, Clone)]
pub enum DecodeError {
    BufferLength,
}

pub trait BufWriter {
    fn put_slice(&mut self, slice: &[u8]);
    fn put_u8(&mut self, val: u8);
    fn put_u32(&mut self, val: u32);
    fn put_u64(&mut self, val: u64);
}

pub trait BufReader {
    fn get_slice(&mut self, size: usize) -> Result<&[u8], DecodeError>;
    fn get_u8(&mut self) -> Result<u8, DecodeError>;
    fn get_u32(&mut self) -> Result<u32, DecodeError>;
    fn get_u64(&mut self) -> Result<u64, DecodeError>;
    fn get_array<const C: usize>(&mut self) -> Result<[u8; C], DecodeError>;
    fn get_into_array<const C: usize>(&mut self, buf: &mut [u8; C], amount: usize) -> Result<(), DecodeError>;
    fn position(&self) -> usize;
}

// A simple BufWriter which copies into a vector.
pub struct VectorBufWriter<'a> {
    vec: &'a mut Vec<u8>,
}
impl<'a> VectorBufWriter<'a> {
    pub fn new(vec: &'a mut Vec<u8>) -> Self {
        Self { vec: vec }
    }
}

macro_rules! impl_slice_write {
    ($f_name:ident, $int:ident) => {
        fn $f_name(&mut self, val: $int) {
            self.vec.extend_from_slice(&val.to_le_bytes()[..]);
        }
    };
}
impl<'a> BufWriter for VectorBufWriter<'a> {
    fn put_slice(&mut self, slice: &[u8]) {
        self.vec.extend_from_slice(slice);
    }

    impl_slice_write!(put_u8, u8);
    impl_slice_write!(put_u32, u32);
    impl_slice_write!(put_u64, u64);
}

macro_rules! impl_slice_read {
    ($f_name:ident, $int:ident) => {
        fn $f_name(&mut self) -> Result<$int, DecodeError> {
            if self.bytes.len() - self.pos < std::mem::size_of::<$int>() {
                return Err(DecodeError::BufferLength);
            }
            let mut buf = [0; std::mem::size_of::<$int>()];
            buf.copy_from_slice(&self.bytes[self.pos..self.pos + std::mem::size_of::<$int>()]);
            let val = $int::from_le_bytes(buf);
            self.pos = self.pos + std::mem::size_of::<$int>();
            Ok(val)
        }
    };
}

// Simple BufReader for pulling our data out of a slice of bytes.
pub struct SliceReader<'a> {
    pos: usize,
    bytes: &'a [u8],
}
impl<'a> SliceReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { pos: 0, bytes }
    }
}

impl<'a> BufReader for SliceReader<'a> {
    fn get_slice(&mut self, size: usize) -> Result<&[u8], DecodeError> {
        if self.bytes.len() - self.pos < size {
            return Err(DecodeError::BufferLength);
        }
        let sl = &self.bytes[self.pos..self.pos + size];
        self.pos = self.pos + size;
        Ok(sl)
    }

    impl_slice_read!(get_u8, u8);
    impl_slice_read!(get_u32, u32);
    impl_slice_read!(get_u64, u64);

    fn get_array<const C: usize>(&mut self) -> Result<[u8; C], DecodeError> {
        if self.bytes.len() - self.pos < C {
            return Err(DecodeError::BufferLength);
        }
        let mut buf: [u8; C] = [0; C];
        self.get_into_array(&mut buf, C)?;
        Ok(buf)
    }

    fn get_into_array<const C: usize>(&mut self, buf: &mut [u8; C], amount: usize) -> Result<(), DecodeError> {
        if self.bytes.len() - self.pos < amount {
            return Err(DecodeError::BufferLength);
        }
        buf.copy_from_slice(&self.bytes[self.pos..self.pos + amount]);
        Ok(())
    }

    fn position(&self) -> usize {
        self.pos
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::{BufReader, BufWriter, SliceReader, VectorBufWriter};

    #[test]
    fn test_simple_encode_decode() {
        let mut buffer: Vec<u8> = vec![];
        let mut writer = VectorBufWriter::new(&mut buffer);
        writer.put_u8(5);
        writer.put_u32(6);
        writer.put_u64(7);

        let arr_val = [1, 2, 3, 4];
        writer.put_slice(&arr_val[..]);

        let mut reader = SliceReader::new(buffer.as_slice());
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
