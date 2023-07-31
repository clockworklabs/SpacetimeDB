use super::write::Write;

// aka Record
// Must be atomically, durably written to disk
#[derive(Debug, Clone)]
pub struct Transaction {
    pub writes: Vec<Write>,
}

// tx: [<write>...(dedupped and sorted_numerically)]*
impl Transaction {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = &mut bytes.as_ref();
        if bytes.is_empty() {
            return (Transaction { writes: Vec::new() }, 0);
        }

        let mut bytes_read = 0;

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[bytes_read..bytes_read + 4]);
        let writes_count = u32::from_le_bytes(dst);
        bytes_read += 4;

        let mut writes: Vec<Write> = Vec::with_capacity(writes_count as usize);

        let mut count = 0;
        while bytes_read < bytes.len() && count < writes_count {
            let (write, read) = Write::decode(&bytes[bytes_read..]);
            bytes_read += read;
            writes.push(write);
            count += 1;
        }

        (Transaction { writes }, bytes_read)
    }

    pub fn encoded_len(&self) -> usize {
        let mut count = 4;
        for write in &self.writes {
            count += write.encoded_len();
        }
        count
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.extend((self.writes.len() as u32).to_le_bytes());

        for write in &self.writes {
            write.encode(bytes);
        }
    }
}
