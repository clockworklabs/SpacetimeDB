use super::super::hash::Hash;

pub struct Commit {
    pub parent_commit: Hash,
    pub writes: Vec<Write>,
}

pub struct Transaction {
    pub parent_commit: Hash,
    pub reads: Vec<Read>,
    pub writes: Vec<Write>,
}

// 12 bytes
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Read {
    table_id: u32,
    row_key: u64,
}

// 36 bytes
#[derive(Debug, Copy, Clone)]
pub enum Write {
    Insert { table_id: u32, content: [u8; 32] },
    Delete { table_id: u32, row_key: u64 },
}

