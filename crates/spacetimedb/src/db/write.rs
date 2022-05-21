use crate::hash::Hash;

#[derive(Debug, Copy, Clone)]
pub enum Write {
    Insert { set_id: u32, hash: Hash },
    Delete { set_id: u32, hash: Hash },
}

impl Write {
    // write: <write_type(1)><hash(32)>
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = &mut bytes.as_ref();

        let op = bytes[0];
        *bytes = &bytes[1..];

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        let set_id = u32::from_le_bytes(dst);
        *bytes = &bytes[4..];

        let mut hash = Hash::default();
        hash.copy_from_slice(&bytes[..32]);
        *bytes = &bytes[32..];

        if op == 0 {
            (Write::Delete { set_id, hash }, 37)
        } else {
            (Write::Insert { set_id, hash }, 37)
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            Write::Insert { set_id, hash } => {
                bytes.push(1);
                bytes.extend(set_id.to_le_bytes());
                bytes.extend(hash);
            }
            Write::Delete { set_id, hash } => {
                bytes.push(0);
                bytes.extend(set_id.to_le_bytes());
                bytes.extend(hash);
            }
        }
    }
}
