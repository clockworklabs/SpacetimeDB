use super::transaction::Transaction;
use std::sync::Arc;

// aka "Block" from blockchain, aka RecordBatch, aka TxBatch
#[derive(Debug)]
pub struct Commit {
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
                    transactions: Vec::new(),
                },
                0,
            );
        }

        let mut read_count = 0;

        let mut dst = [0u8; 8];
        dst.copy_from_slice(&bytes[read_count..read_count + 8]);

        let mut dst = [0u8; 8];
        dst.copy_from_slice(&bytes[read_count..read_count + 8]);

        let mut transactions: Vec<Arc<Transaction>> = Vec::new();
        while read_count < bytes.len() {
            let (tx, read) = Transaction::decode(&bytes[read_count..]);
            read_count += read;
            transactions.push(Arc::new(tx));
        }

        (Commit { transactions }, read_count)
    }

    pub fn encoded_len(&self) -> usize {
        let mut count = 0;

        for tx in &self.transactions {
            count += tx.encoded_len();
        }

        count
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.reserve(self.encoded_len());

        for tx in &self.transactions {
            tx.encode(bytes);
        }
    }
}
