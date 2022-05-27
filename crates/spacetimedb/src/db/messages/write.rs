use crate::hash::{hash_bytes, Hash};

#[derive(Debug, Copy, Clone)]
pub struct Write {
    pub operation: Operation,
    pub set_id: u32,
    pub value: Value,
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum Operation {
    Delete = 0,
    Insert,
}

impl Operation {
    pub fn to_u8(&self) -> u8 {
        match self {
            Operation::Delete => 0,
            Operation::Insert => 1,
        }
    }

    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Delete,
            _ => Self::Insert,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    Data { len: u8, buf: [u8; 32] },
    Hash(Hash),
}

// <flags(1)><value(0-32)>
impl Value {
    // Convert a bunch of data to the value that represents it.
    // Throws away the data.
    pub fn from_data(bytes: impl AsRef<[u8]>) -> Self {
        let bytes = bytes.as_ref();
        if bytes.len() > 32 {
            Value::Hash(hash_bytes(&bytes))
        } else {
            let mut buf = [0; 32];
            buf[0..bytes.len()].copy_from_slice(&bytes[0..bytes.len()]);
            Value::Data {
                len: bytes.len() as u8,
                buf,
            }
        }
    }

    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = &mut bytes.as_ref();
        let mut read_count = 0;

        let flags = bytes[read_count];
        read_count += 1;

        let is_data = ((flags & 0b1000_0000) >> 7) == 0;

        if is_data {
            let len = flags & 0b0111_1111;
            let mut buf = [0; 32];
            buf[0..len as usize].copy_from_slice(&bytes[read_count..(read_count + len as usize)]);
            read_count += len as usize;
            (Self::Data { len, buf }, read_count)
        } else {
            let hash = Hash::from_slice(&bytes[read_count..read_count + 32]);
            read_count += 32;
            (Self::Hash(*hash), read_count)
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) -> usize {
        match self {
            Value::Data { len, buf } => {
                let flags: u8 = 0b0000_0000;
                let flags = flags | (len & 0b0111_1111);
                bytes.push(flags);
                bytes.extend(&buf[0..(*len as usize)]);
                1 + *len as usize
            }
            Value::Hash(hash) => {
                let flags: u8 = 0b1000_0000;
                bytes.push(flags);
                bytes.extend(hash);
                33
            }
        }
    }
}

impl Write {
    // write_flags:
    // b0 = insert/delete,
    // b1 = unused,
    // b2 = unused,
    // b3,b4,b5,b6,b7 unused
    // write: <write_flags(1)><set_id(4)><value(1-33)>
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = &mut bytes.as_ref();
        let mut read_count = 0;

        let flags = bytes[read_count];
        read_count += 1;

        let op = (flags & 0b1000_0000) >> 7;

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[read_count..read_count + 4]);
        let set_id = u32::from_le_bytes(dst);
        read_count += 4;

        let (value, rc) = Value::decode(&bytes[read_count..]);
        read_count += rc;

        (
            Write {
                operation: Operation::from_u8(op),
                set_id,
                value,
            },
            read_count,
        )
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        let mut flags: u8 = 0;
        flags = if self.operation.to_u8() != 0 {
            flags | 0b1000_0000
        } else {
            flags
        };
        bytes.push(flags);
        bytes.extend(self.set_id.to_le_bytes());
        self.value.encode(bytes);
    }
}
