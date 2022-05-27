use super::transaction::Transaction;
use crate::hash::Hash;

// aka "Block" from blockchain, aka RecordBatch, aka TxBatch
#[derive(Debug)]
pub struct Commit {
    pub parent_commit_hash: Option<Hash>,
    pub commit_offset: u64,
    pub min_tx_offset: u64,
    pub transactions: Vec<Transaction>,
}

// TODO: Maybe a transaction buffer hash?
// commit: <parent_commit_hash(32)><commit_offset(8)><min_tx_offset(8)>[<transaction>...]*
impl Commit {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = &mut bytes.as_ref();
        if bytes.len() == 0 {
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
            let mut parent_commit_hash = Hash::default();
            parent_commit_hash.copy_from_slice(&bytes[read_count..read_count + 32]);
            read_count += 32;
            Some(parent_commit_hash)
        } else {
            read_count += 1;
            None
        };

        let mut dst = [0u8; 8];
        dst.copy_from_slice(&bytes[read_count..read_count + 8]);
        let min_tx_offset = u64::from_le_bytes(dst);
        read_count += 8;

        let mut dst = [0u8; 8];
        dst.copy_from_slice(&bytes[read_count..read_count + 8]);
        let commit_offset = u64::from_le_bytes(dst);
        read_count += 8;

        let mut transactions: Vec<Transaction> = Vec::new();
        while read_count < bytes.len() {
            let (tx, read) = Transaction::decode(&bytes[read_count..]);
            read_count += read;
            transactions.push(tx);
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

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        if self.parent_commit_hash.is_none() {
            bytes.push(0);
        } else {
            bytes.push(1);
            bytes.extend(self.parent_commit_hash.unwrap());
        }

        bytes.extend(self.commit_offset.to_le_bytes());
        bytes.extend(self.min_tx_offset.to_le_bytes());

        for tx in &self.transactions {
            tx.encode(bytes);
        }
    }
}
