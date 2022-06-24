use super::hash::Hash;
use crate::hash::hash_bytes;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum DataKey {
    Data { len: u8, buf: [u8; 32] },
    Hash(Hash),
}

// <flags(1)><value(0-32)>
impl DataKey {
    // Convert a bunch of data to the value that represents it.
    // Throws away the data.
    pub fn from_data(bytes: impl AsRef<[u8]>) -> Self {
        let bytes = bytes.as_ref();
        if bytes.len() > 32 {
            DataKey::Hash(hash_bytes(&bytes))
        } else {
            let mut buf = [0; 32];
            buf[0..bytes.len()].copy_from_slice(&bytes[0..bytes.len()]);
            DataKey::Data {
                len: bytes.len() as u8,
                buf,
            }
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data_key_summary = Vec::new();
        self.encode(&mut data_key_summary);
        data_key_summary
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
            DataKey::Data { len, buf } => {
                let flags: u8 = 0b0000_0000;
                let flags = flags | (len & 0b0111_1111);
                bytes.push(flags);
                bytes.extend(&buf[0..(*len as usize)]);
                1 + *len as usize
            }
            DataKey::Hash(hash) => {
                let flags: u8 = 0b1000_0000;
                bytes.push(flags);
                bytes.extend(hash);
                33
            }
        }
    }
}
