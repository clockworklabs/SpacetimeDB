pub use spacetimedb_sats::DataKey;

#[derive(Debug, Copy, Clone)]
pub struct Write {
    pub operation: Operation,
    pub set_id: u32,
    pub data_key: DataKey,
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

        let mut reader = &bytes[read_count..];
        let orig_len = reader.len();
        let value = DataKey::decode(&mut reader).unwrap();
        read_count += orig_len - reader.len();

        (
            Write {
                operation: Operation::from_u8(op),
                set_id,
                data_key: value,
            },
            read_count,
        )
    }

    pub fn encoded_len(&self) -> usize {
        // 1 for flags, 4 for set_id
        let mut count = 1 + 4;
        count += self.data_key.encoded_len();
        count
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
        self.data_key.encode(bytes);
    }
}
