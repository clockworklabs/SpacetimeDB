use crate::hash::Hash;

use super::write::Write;

#[derive(Debug)]
pub struct Commit {
    parent_commit_hash: Option<Hash>,
    writes: Vec<Write>,
}

impl Commit {
    // commit: <parent_commit_hash(32)>[<table_update>...(dedupped and sorted_numerically)]*
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = &mut bytes.as_ref();
        if bytes.len() == 0 {
            return (
                Commit {
                    parent_commit_hash: None,
                    writes: Vec::new(),
                },
                0,
            );
        }

        let mut start = 0;
        let mut parent_commit_hash = Hash::default();
        parent_commit_hash.copy_from_slice(&bytes[start..start + 32]);
        start += 32;

        let mut writes: Vec<Write> = Vec::new();
        while bytes.len() > 0 {
            let (write, read) = Write::decode(&bytes[start..]);
            start += read;
            writes.push(write);
        }

        (
            Commit {
                parent_commit_hash: Some(parent_commit_hash),
                writes,
            },
            start,
        )
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        if self.parent_commit_hash.is_none() {
            return;
        }

        if let Some(parent_commit_hash) = self.parent_commit_hash {
            for byte in parent_commit_hash {
                bytes.push(byte);
            }
        }

        for update in &self.writes {
            update.encode(bytes);
        }
    }
}
