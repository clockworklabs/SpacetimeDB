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
        if bytes.len() == 0 {
            return (Transaction { writes: Vec::new() }, 0);
        }

        let mut bytes_read = 0;

        let mut writes: Vec<Write> = Vec::new();
        while bytes_read < bytes.len() {
            let (write, read) = Write::decode(&bytes[bytes_read..]);
            bytes_read += read;
            writes.push(write);
        }

        (Transaction { writes }, bytes_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        for write in &self.writes {
            write.encode(bytes);
        }
    }
}
