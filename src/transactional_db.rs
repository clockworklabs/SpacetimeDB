use std::collections::HashSet;
use sha3::digest::{generic_array::GenericArray, generic_array::typenum::U32};
use crate::{hash::hash_bytes, messages::Commit, object_db::ObjectDB};
// TODO: maybe use serde?

type Hash = GenericArray<u8, U32>;

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}

pub struct Transaction {
    parent_commit: Hash,
    table_reads: Vec<TableRead>,
    table_updates: Vec<TableUpdate>
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct TableRead {
    table_id: u32,
    tuple_hash: Hash,
}

#[derive(Debug, Copy, Clone)]
enum Write {
    Insert(Hash),
    Delete(Hash),
}

impl Write {
    // write: <write_type(1)><tuple_hash(32)>
    fn decode(bytes: &mut &[u8]) -> Self {
        let start = 0;
        let end = 33;
        let op = bytes[start];
        let mut hash = Hash::default();
        hash.copy_from_slice(&bytes[start+1..end]);
        *bytes = &bytes[end..];

        if op == 0 {
            Write::Delete(hash)
        } else {
            Write::Insert(hash)
        }
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            Write::Insert(hash) => {
                bytes.push(1);
                for byte in hash {
                    bytes.push(*byte);
                }
            },
            Write::Delete(hash) => {
                bytes.push(0);
                for byte in hash {
                    bytes.push(*byte);
                }
            },
        }
    }
}

#[derive(Debug)]
struct TableUpdate {
    table_id: u32,
    writes: Vec<Write>
}

impl TableUpdate {
    // table_update: <table_id(4)><length_bytes(4)>[<write(N)>...]*
    fn decode(bytes: &mut &[u8]) -> Self {
        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        *bytes = &bytes[4..];

        let table_id = u32::from_be_bytes(dst);

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        *bytes = &bytes[4..];

        // TODO: do we not need length here?
        let _length = u32::from_be_bytes(dst);

        let mut writes: Vec<Write> = Vec::new();
        while bytes.len() > 0 {
            writes.push(Write::decode(bytes));
        }

        Self {
            table_id,
            writes
        }
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.extend(self.table_id.to_be_bytes());

        let num_writes = self.writes.len();
        let size_of_write = 33; // TODO
        bytes.extend(((num_writes * size_of_write) as u32).to_be_bytes());

        for write in &self.writes {
            write.encode(bytes);
        }
    }
}

#[derive(Debug)]
struct CommitObj {
    parent_commit_hash: Option<Hash>,
    table_updates: Vec<TableUpdate>,
}

impl CommitObj {

    // commit: <parent_commit_hash(32)>[<table_update>...(dedupped and sorted_numerically)]*
    fn decode(bytes: &mut &[u8]) -> Self {
        if bytes.len() == 0 {
            return CommitObj {
                parent_commit_hash: None,
                table_updates: Vec::new(),
            }
        }

        let start = 0;
        let end = 32;
        let mut parent_commit_hash = Hash::default();
        parent_commit_hash.copy_from_slice(&bytes[start..end]);

        *bytes = &bytes[end..];
        let mut table_updates: Vec<TableUpdate> = Vec::new();
        while bytes.len() > 0 {
            table_updates.push(TableUpdate::decode(bytes));
        }

        CommitObj {
            parent_commit_hash: Some(parent_commit_hash),
            table_updates,
        }
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        if self.parent_commit_hash.is_none() {
            return;
        }

        if let Some(parent_commit_hash) = self.parent_commit_hash {
            for byte in parent_commit_hash {
                bytes.push(byte);
            }
        }

        for update in &self.table_updates {
            update.encode(bytes);
        }
    }

}

// Insert: <table_id><tuple>
// Delete: <table_id><tuple_hash>
pub struct Table {
    id: u32,
    name: String,
    closed_state: HashSet<Hash>,
}

pub struct TransactionalDB {
    pub odb: ObjectDB,
    tables: Vec<Table>,
    closed_commit: Hash,
    open_commits: Vec<Hash>,
    branched_commits: Vec<Hash>,
}

impl TransactionalDB {
    pub fn new() -> Self {
        let commit = CommitObj {
            parent_commit_hash: None,
            table_updates: Vec::new(),
        };

        let mut odb = ObjectDB::new();

        let mut initial_commit_bytes: Vec<u8> = Vec::new();
        commit.encode(&mut initial_commit_bytes);
        let commit_hash = odb.add(initial_commit_bytes);

        Self {
            odb,
            closed_commit: commit_hash,
            branched_commits: Vec::new(),
            open_commits: Vec::new(),
            tables: Vec::new(),
        }
    }

    fn latest_commit(&self) -> Hash {
        self.open_commits.last().map(|h| *h).unwrap_or(self.closed_commit)
    }

    pub fn begin_tx(&mut self) -> Transaction {
        let parent = self.latest_commit();
        self.branched_commits.push(parent);
        Transaction {
            parent_commit: parent,
            table_reads: Vec::new(),
            table_updates: Vec::new(),
        }
    }

    pub fn commit_tx(&mut self, tx: Transaction) -> bool {
        if self.latest_commit() == tx.parent_commit {
            self.finalize(tx);
            return true;
        }

        // If not, we need to merge.
        // - If I did not read something that someone else wrote to, we're good to just merge
        // in my changes, because the part of the DB I read from is still current.
        // - If I did read something someone else wrote to, then the issue is that I am
        // potentially basing my changes off of stuff that has changed since I last pulled.
        // I should pull and then try to reapply my changes based on the most current state
        // of the database (basically rebase off latest_commit). Really this just means the
        // transaction has failed, and the transaction should be tried again with a a new
        // parent commit.
        let mut read_set: HashSet<TableRead> = HashSet::new();
        for read in &tx.table_reads {
            read_set.insert(*read);
        }

        let mut commit_hash = self.latest_commit();
        loop {
            let commit_obj = CommitObj::decode(&mut self.odb.get(commit_hash).unwrap());
            for table_update in commit_obj.table_updates {
                for write in table_update.writes {
                    let hash = match write {
                        Write::Insert(hash) => hash,
                        Write::Delete(hash) => hash,
                    };
                    if read_set.contains(&TableRead { table_id: table_update.table_id, tuple_hash: hash}) {
                        return false;
                    }
                }
            }

            if commit_obj.parent_commit_hash == Some(tx.parent_commit) {
                break;
            }

            commit_hash = commit_obj.parent_commit_hash.unwrap();
        }

        self.finalize(tx);
        true
    }

    fn finalize(&mut self, tx: Transaction) {
        // Rebase on the last open commit (or closed commit if none open)
        let new_commit = CommitObj {
            parent_commit_hash: Some(self.latest_commit()),
            table_updates: tx.table_updates,
        };

        let mut commit_bytes = Vec::new();
        new_commit.encode(&mut commit_bytes);
        let commit_hash = hash_bytes(&commit_bytes);

        self.odb.add(commit_bytes);
        self.open_commits.push(commit_hash);

        // Remove my branch
        let index = self.branched_commits.iter().position(|hash| *hash == tx.parent_commit).unwrap();
        self.branched_commits.swap_remove(index);

        if tx.parent_commit == self.closed_commit {
            let index = self.branched_commits.iter().position(|hash| *hash == tx.parent_commit);
            if index == None {
                // I was the last branch preventing closing of the next open commit.
                // Close the open commits until the next branch.
                loop {
                    let next_open = self.open_commits.first().map(|h| *h);
                    if let Some(next_open) = next_open {
                        self.closed_commit = next_open;
                        self.open_commits.remove(0);

                        let commit_obj = CommitObj::decode(&mut self.odb.get(next_open).unwrap());
                        for table_update in commit_obj.table_updates {
                            // TODO: index the tables by id
                            let table = self.tables.iter_mut().find(|t| t.id == table_update.table_id).unwrap();
                            for write in table_update.writes {
                                match write {
                                    Write::Insert(hash) => {
                                        table.closed_state.insert(hash);
                                    },
                                    Write::Delete(hash) => {
                                        table.closed_state.remove(&hash);
                                    },
                                }
                            }
                        }
                        // If someone branched off of me, we're done otherwise continue
                        if self.branched_commits.contains(&next_open) {
                            break;
                        }
                    } else {
                        // No more commits to process
                        break;
                    }
                }
            }
        }
    }

    pub fn seek(&self, tx: &mut Transaction, table_id: u32, hash: Hash) -> Option<&[u8]> {
        // I'm not sure if this just needs to track reads from the parent commit
        // or reads from the transaction as well.
        tx.table_reads.push(TableRead { table_id, tuple_hash: hash });

        // Even uncommitted rows will be in the odb. This will accumulate garbage over time,
        // but we could also clear it if a commit fails (or store uncommited changes in a different odb).
        // You could potentially check if this is None to short circuit things, but that's
        // only if you're sure everything is in the odb.
        let row_obj = self.odb.get(hash);

        // Search back through this transaction
        let table_update = tx.table_updates.iter().find(|t| t.table_id == table_id);
        if let Some(table_update) = table_update {
            for i in (0..table_update.writes.len()).rev() {
                match &table_update.writes[i] {
                    Write::Insert(h) => {
                        if *h == hash {
                            return Some(row_obj.unwrap());
                        } 
                    },
                    Write::Delete(h) => {
                        if *h == hash {
                            return None;
                        }
                    },
                };
            }
        }

        // Search backwards through all open commits that are parents of this transaction.
        // if you find a delete it's not there.
        // if you find an insert it is there. If you find no mention of it, then whether
        // it's there or not is dependent on the closed_state.
        let mut i = self.open_commits.iter().position(|h| *h == tx.parent_commit).unwrap_or(0);
        loop {
            let next_open = self.open_commits.get(i).map(|h| *h);
            if let Some(next_open) = next_open {
                let commit_obj = CommitObj::decode(&mut self.odb.get(next_open).unwrap());
                let table_update = commit_obj.table_updates.iter().find(|t| t.table_id == table_id).unwrap();
                for write in &table_update.writes {
                    match write {
                        Write::Insert(h) => {
                            if *h == hash {
                                return row_obj;
                            }
                        },
                        Write::Delete(h) => {
                            if *h == hash {
                                return None;
                            }
                        },
                    }
                }
            } else {
                // No more commits to process
                break;
            }
            i -= 1;
        }

        let table = self.tables.iter().find(|t| t.id == table_id).unwrap();
        if table.closed_state.contains(&hash) {
            return Some(row_obj.unwrap());
        }

        None
    }

    pub fn delete(&mut self, tx: &mut Transaction, table_id: u32, hash: Hash) {
        for table_update in &mut tx.table_updates {
            if table_update.table_id == table_id {
                table_update.writes.push(Write::Delete(hash));
                return;
            }
        }
        tx.table_updates.push(TableUpdate {
            table_id,
            writes: vec![Write::Delete(hash)],
        });
    }

    pub fn insert(&mut self, tx: &mut Transaction, table_id: u32, bytes: Vec<u8>) -> Hash {
        // Add bytes to the odb
        let hash = hash_bytes(&bytes);
        self.odb.add(bytes);

        for table_update in &mut tx.table_updates {
            if table_update.table_id == table_id {
                table_update.writes.push(Write::Insert(hash));
                return hash;
            }
        }
        tx.table_updates.push(TableUpdate {
            table_id,
            writes: vec![Write::Insert(hash)],
        });

        hash
    }

    // TODO: DDL (domain definition language) statements should also be transactional maybe
    pub fn create_table(&mut self, name: &str) -> u32 {
        // TODO: doesn't work when tables can be removed
        let id = self.tables.len() as u32;
        self.tables.push(Table {
            id,
            name: name.to_owned(),
            closed_state: HashSet::new(),
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::TransactionalDB;

    #[test]
    fn test_insert_and_seek() {
        let mut db = TransactionalDB::new();
        let mut tx = db.begin_tx();
        let table = db.create_table("my_table");
        let row_key_1 = db.insert(&mut tx, table, b"this is a byte string".to_vec());
        db.commit_tx(tx);

        let mut tx = db.begin_tx();
        let row = db.seek(&mut tx, table, row_key_1).unwrap();

        assert_eq!(b"this is a byte string", row);
    }

    // #[test]
    // fn test_read_isolation() {
    //     let mut table = Table::new();
    //     let mut tx_1 = table.begin_tx();
    //     let row_key_1 = table.insert(&mut tx_1, MyRowObj {
    //         my_name: hash_bytes(b"This is a byte string."),
    //         my_i32: -1,
    //         my_u64: 1,
    //         my_hash: hash_bytes(b"This will be turned into a hash."),
    //     });

    //     let mut tx_2 = table.begin_tx();
    //     let row = table.seek(&mut tx_2, row_key_1);
    //     assert!(row.is_none());
        
    //     table.commit_tx(tx_1);
        
    //     let mut tx_3 = table.begin_tx();
    //     let row = table.seek(&mut tx_3, row_key_1);
    //     assert!(row.is_some());
    // }

    // #[test]
    // fn test_write_skew_conflict() {
    //     let mut table = Table::new();
    //     let mut tx_1 = table.begin_tx();
    //     let row_key_1 = table.insert(&mut tx_1, MyRowObj {
    //         my_name: hash_bytes(b"This is a byte string."),
    //         my_i32: -1,
    //         my_u64: 1,
    //         my_hash: hash_bytes(b"This will be turned into a hash."),
    //     });

    //     let mut tx_2 = table.begin_tx();
    //     let row = table.seek(&mut tx_2, row_key_1);
    //     assert!(row.is_none());
        
    //     assert!(table.commit_tx(tx_1));
    //     assert!(!table.commit_tx(tx_2));
    // }

    // #[test]
    // fn test_write_skew_no_conflict() {
    //     let mut table = Table::new();
    //     let mut tx_1 = table.begin_tx();
    //     let row_key_1 = table.insert(&mut tx_1, MyRowObj {
    //         my_name: hash_bytes(b"This is a byte string."),
    //         my_i32: -1,
    //         my_u64: 1,
    //         my_hash: hash_bytes(b"This will be turned into a hash."),
    //     });
    //     let row_key_2 = table.insert(&mut tx_1, MyRowObj {
    //         my_name: hash_bytes(b"This is a byte string."),
    //         my_i32: -2,
    //         my_u64: 1,
    //         my_hash: hash_bytes(b"This will be turned into a hash."),
    //     });
    //     assert!(table.commit_tx(tx_1));

    //     let mut tx_2 = table.begin_tx();
    //     let row = table.seek(&mut tx_2, row_key_1);
    //     assert!(row.is_some());
    //     table.delete(&mut tx_2, row_key_2);
        
    //     let mut tx_3 = table.begin_tx();
    //     table.delete(&mut tx_3, row_key_1);
        
    //     assert!(table.commit_tx(tx_2));
    //     assert!(table.commit_tx(tx_3));
    // }

    // #[test]
    // fn test_size() {
    //     let start = std::time::Instant::now();
    //     let mut table = Table::new();
    //     for i in 0..1000 {
    //         let mut tx_1 = table.begin_tx();
    //         table.insert(&mut tx_1, MyRowObj {
    //             my_name: hash_bytes(b"This is a byte string."),
    //             my_i32: -i,
    //             my_u64: i as u64,
    //             my_hash: hash_bytes(b"This will be turned into a hash."),
    //         });
    //         table.insert(&mut tx_1, MyRowObj {
    //             my_name: hash_bytes(b"This is a byte string."),
    //             my_i32: -2 * i,
    //             my_u64: i as u64,
    //             my_hash: hash_bytes(b"This will be turned into a hash."),
    //         });
    //         assert!(table.commit_tx(tx_1));
    //     }
    //     let duration = start.elapsed();

    //     println!("{}", table.odb.total_mem_size_bytes());
    //     println!("{}", duration.as_millis());
    // }

}


