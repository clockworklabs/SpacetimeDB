use super::transaction::Transaction;
use crate::hash::Hash;
use std::sync::Arc;

// aka "Block" from blockchain, aka RecordBatch, aka TxBatch
#[derive(Debug)]
pub struct Commit {
    pub parent_commit_hash: Option<Hash>,
    pub commit_offset: u64,
    pub min_tx_offset: u64,
    pub transactions: Vec<Arc<Transaction>>,
}

// TODO: Maybe a transaction buffer hash?
// commit: <parent_commit_hash(32)><commit_offset(8)><min_tx_offset(8)>[<transaction>...]*
impl Commit {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = &mut bytes.as_ref();
        if bytes.is_empty() {
            return (
                Commit {
                    parent_commit_hash: None,
                    commit_offset: 0,
                    min_tx_offset: 0,
                    transactions: Vec::new(),
                },
                0,
            );
        }

        let mut read_count = 0;

        let parent_commit_hash = if bytes[read_count] != 0 {
            read_count += 1;
            let parent_commit_hash = Hash::from_slice(&bytes[read_count..read_count + 32]);
            read_count += 32;
            Some(parent_commit_hash)
        } else {
            read_count += 1;
            None
        };

        let mut dst = [0u8; 8];
        dst.copy_from_slice(&bytes[read_count..read_count + 8]);
        let commit_offset = u64::from_le_bytes(dst);
        read_count += 8;

        let mut dst = [0u8; 8];
        dst.copy_from_slice(&bytes[read_count..read_count + 8]);
        let min_tx_offset = u64::from_le_bytes(dst);
        read_count += 8;

        let mut transactions: Vec<Arc<Transaction>> = Vec::new();
        while read_count < bytes.len() {
            let (tx, read) = Transaction::decode(&bytes[read_count..]);
            read_count += read;
            transactions.push(Arc::new(tx));
        }

        (
            Commit {
                parent_commit_hash,
                commit_offset,
                min_tx_offset,
                transactions,
            },
            read_count,
        )
    }

    pub fn encoded_len(&self) -> usize {
        let mut count = 0;

        if self.parent_commit_hash.is_none() {
            count += 1;
        } else {
            count += 1;
            count += self.parent_commit_hash.unwrap().data.len();
        }

        // 8 for commit_offset
        count += 8;

        // 8 for min_tx_offset
        count += 8;

        for tx in &self.transactions {
            count += tx.encoded_len();
        }

        count
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.reserve(self.encoded_len());

        if self.parent_commit_hash.is_none() {
            bytes.push(0);
        } else {
            bytes.push(1);
            bytes.extend(self.parent_commit_hash.unwrap().data);
        }

        bytes.extend(self.commit_offset.to_le_bytes());
        bytes.extend(self.min_tx_offset.to_le_bytes());

        for tx in &self.transactions {
            tx.encode(bytes);
        }
    }
}
