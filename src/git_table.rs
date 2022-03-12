use std::collections::HashSet;
use crate::object_db::ObjectDB;
use crate::hash::{Hash, hash_bytes};

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct MyRowObj {
    my_name: Hash,
    my_i32: i32,
    my_u64: u64,
    my_hash: Hash
}

enum Write {
    Insert(Hash),
    Delete(Hash),
}

pub struct Transaction {
    parent_commit: Hash,
    reads: Vec<Hash>,
    writes: Vec<Write>
}

struct CommitObj {
    parent_commit_hash: Option<Hash>,
    table_hash: Hash,
}

struct TableObj {
    row_hashes: Vec<Hash>
}

pub struct Table {
    odb: ObjectDB,
    latest_commit: Hash
}

impl Table {
    pub fn new() -> Self {
        let mut odb = ObjectDB::new();
        let table_bytes = Self::encode_table(TableObj {
            row_hashes: Vec::new(),
        });
        let table_hash = odb.add(table_bytes);
        let initial_commit_bytes = Self::encode_commit(CommitObj {
            parent_commit_hash: None,
            table_hash,
        });
        let commit_hash = odb.add(initial_commit_bytes);
        Self {
            odb,
            latest_commit: commit_hash,
        }
    }

    fn decode_commit(bytes: &[u8]) -> CommitObj {
        if bytes.len() == 32 {
            let mut table_hash = Hash::default();
            table_hash.clone_from_slice(&bytes[0..32]);
            return CommitObj {
                parent_commit_hash: None,
                table_hash,
            }
        }
        assert_eq!(bytes.len(), 64);
        let mut parent_commit_hash = Hash::default();
        parent_commit_hash.clone_from_slice(&bytes[0..32]);
        let mut table_hash = Hash::default();
        table_hash.clone_from_slice(&bytes[32..64]);
        CommitObj {
            parent_commit_hash: Some(parent_commit_hash),
            table_hash,
        }
    }

    fn decode_table(bytes: &[u8]) -> TableObj {
        let mut curr = 0;
        let mut row_hashes = Vec::new();
        while curr < bytes.len() {
            let mut hash = Hash::default();
            hash.clone_from_slice(&bytes[curr..curr+32]);
            row_hashes.push(hash);
            curr += 32;
        }
        TableObj {
            row_hashes,
        }
    }

    fn decode_row(bytes: &[u8]) -> MyRowObj {
        unsafe { std::ptr::read(bytes.as_ptr() as *const _) }
    }
    
    fn encode_commit(commit: CommitObj) -> Vec<u8> {
        let mut commit_bytes = Vec::new();
        commit_bytes.reserve(64);
        if let Some(parent_commit_hash) = commit.parent_commit_hash {
            for byte in parent_commit_hash {
                commit_bytes.push(byte);
            }
        }
        for byte in commit.table_hash {
            commit_bytes.push(byte);
        }
        return commit_bytes;
    }

    fn encode_table(table: TableObj) -> Vec<u8> {
        let mut table_bytes = Vec::new();
        for hash in table.row_hashes {
            for byte in hash {
                table_bytes.push(byte);
            }
        }
        return table_bytes;
    }

    fn encode_row(row: MyRowObj) -> Vec<u8> {
        unsafe { any_as_u8_slice(&row) }.to_vec()
    }

    pub fn begin_tx(&self) -> Transaction {
        Transaction {
            parent_commit: self.latest_commit,
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub fn seek(&self, tx: &mut Transaction, hash: Hash) -> Option<MyRowObj> {
        // I'm not sure if this just needs to track reads from the parent commit
        // or reads from the transaction as well.
        tx.reads.push(hash);

        // Even uncommitted rows will be in the odb. This will accumulate garbage over time,
        // but we could also clear it if a commit fails (or store uncommited changes in a different odb).
        // You could potentially check if this is None to short circuit things, but that's
        // only if you're sure everything is in the odb.
        let row_obj = self.odb.get(hash).map(|r| Self::decode_row(r));

        for i in (0..tx.writes.len()).rev() {
            match &tx.writes[i] {
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
        
        let latest_commit_obj = Self::decode_commit(self.odb.get(self.latest_commit).unwrap());
        let latest_table_obj = Self::decode_table(self.odb.get(latest_commit_obj.table_hash).unwrap());

        let mut rows: HashSet<Hash> = HashSet::new();
        rows.extend(latest_table_obj.row_hashes);

        if rows.contains(&hash) {
            return Some(row_obj.unwrap());
        }

        None
    }

    pub fn scan(&self, tx: &mut Transaction, filter: fn(MyRowObj) -> bool) {
        let latest_commit_obj = Self::decode_commit(self.odb.get(self.latest_commit).unwrap());
        let latest_table_obj = Self::decode_table(self.odb.get(latest_commit_obj.table_hash).unwrap());

        let mut rows: HashSet<Hash> = HashSet::new();
        rows.extend(latest_table_obj.row_hashes);
        
        for i in 0..tx.writes.len() {
            match &tx.writes[i] {
                Write::Insert(hash) => rows.insert(*hash),
                Write::Delete(hash) => rows.remove(hash),
            };
        }

        for hash in rows {
            let row_obj = Self::decode_row(self.odb.get(hash).unwrap());
            filter(row_obj);
            tx.reads.push(hash);
        }
    }

    pub fn delete(&mut self, tx: &mut Transaction, hash: Hash) {
        tx.writes.push(Write::Delete(hash));
    }

    pub fn insert(&mut self, tx: &mut Transaction, row: MyRowObj) -> Hash {
        // Add bytes to the odb
        let bytes = Self::encode_row(row);
        let hash = hash_bytes(&bytes);
        self.odb.add(bytes);

        tx.writes.push(Write::Insert(hash));

        hash
    }

    fn finalize(&mut self, tx: &Transaction, mut rows: HashSet<Hash>) {
        for i in 0..tx.writes.len() {
            match tx.writes[i] {
                Write::Insert(hash) => {
                    rows.insert(hash);
                },
                Write::Delete(hash) => {
                    rows.remove(&hash);
                },
            }
        }

        // Create a new table object with the new row
        let table = TableObj {
            row_hashes: Vec::from_iter(rows),
        };
        let table_bytes = Self::encode_table(table);

        let new_commit = CommitObj {
            parent_commit_hash: Some(self.latest_commit),
            table_hash: hash_bytes(&table_bytes),
        };
        let commit_bytes = Self::encode_commit(new_commit);
        let commit_hash = hash_bytes(&commit_bytes);

        self.odb.add(table_bytes);
        self.odb.add(commit_bytes);

        self.latest_commit = commit_hash;
    }

    pub fn commit_tx(&mut self, tx: Transaction) -> bool {
        let latest_commit_obj = Self::decode_commit(self.odb.get(self.latest_commit).unwrap());
        let latest_table_obj = Self::decode_table(self.odb.get(latest_commit_obj.table_hash).unwrap());

        let mut rows: HashSet<Hash> = HashSet::with_capacity(latest_table_obj.row_hashes.len());
        rows.extend(latest_table_obj.row_hashes);

        if self.latest_commit == tx.parent_commit {
            self.finalize(&tx, rows);
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
        let parent_commit_obj = Self::decode_commit(self.odb.get(tx.parent_commit).unwrap());
        
        let parent_table_obj = Self::decode_table(self.odb.get(parent_commit_obj.table_hash).unwrap());
        let mut parent_rows: HashSet<Hash> = HashSet::with_capacity(parent_table_obj.row_hashes.len());
        parent_rows.extend(parent_table_obj.row_hashes);

        // Tyler, this is correct. You might have tried to check for non-existance of a hash
        // during the transaction which was subsequently added.
        let added: HashSet<_> = rows.difference(&parent_rows).collect(); 
        let removed: HashSet<_> = parent_rows.difference(&rows).collect();

        for read in &tx.reads {
            if added.contains(read) || removed.contains(read) {
                return false;
            }
        }

        self.finalize(&tx, rows);

        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Table, hash_bytes, MyRowObj};

    #[test]
    fn test_insert_and_seek() {
        let mut table = Table::new();
        let mut tx = table.begin_tx();
        let row_key_1 = table.insert(&mut tx, MyRowObj {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -1,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        });
        table.commit_tx(tx);

        let mut tx = table.begin_tx();
        let row = table.seek(&mut tx, row_key_1).unwrap();

        let i = row.my_i32;
        assert_eq!(i, -1);
    }

    #[test]
    fn test_read_isolation() {
        let mut table = Table::new();
        let mut tx_1 = table.begin_tx();
        let row_key_1 = table.insert(&mut tx_1, MyRowObj {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -1,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        });

        let mut tx_2 = table.begin_tx();
        let row = table.seek(&mut tx_2, row_key_1);
        assert!(row.is_none());
        
        table.commit_tx(tx_1);
        
        let mut tx_3 = table.begin_tx();
        let row = table.seek(&mut tx_3, row_key_1);
        assert!(row.is_some());
    }

    #[test]
    fn test_write_skew_conflict() {
        let mut table = Table::new();
        let mut tx_1 = table.begin_tx();
        let row_key_1 = table.insert(&mut tx_1, MyRowObj {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -1,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        });

        let mut tx_2 = table.begin_tx();
        let row = table.seek(&mut tx_2, row_key_1);
        assert!(row.is_none());
        
        assert!(table.commit_tx(tx_1));
        assert!(!table.commit_tx(tx_2));
    }

    #[test]
    fn test_write_skew_no_conflict() {
        let mut table = Table::new();
        let mut tx_1 = table.begin_tx();
        let row_key_1 = table.insert(&mut tx_1, MyRowObj {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -1,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        });
        let row_key_2 = table.insert(&mut tx_1, MyRowObj {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -2,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        });
        assert!(table.commit_tx(tx_1));

        let mut tx_2 = table.begin_tx();
        let row = table.seek(&mut tx_2, row_key_1);
        assert!(row.is_some());
        table.delete(&mut tx_2, row_key_2);
        
        let mut tx_3 = table.begin_tx();
        table.delete(&mut tx_3, row_key_1);
        
        assert!(table.commit_tx(tx_2));
        assert!(table.commit_tx(tx_3));
    }

    #[test]
    fn test_size() {
        let start = std::time::Instant::now();
        let mut table = Table::new();
        for i in 0..1000 {
            let mut tx_1 = table.begin_tx();
            table.insert(&mut tx_1, MyRowObj {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -i,
                my_u64: i as u64,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            });
            table.insert(&mut tx_1, MyRowObj {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -2 * i,
                my_u64: i as u64,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            });
            assert!(table.commit_tx(tx_1));
        }
        let duration = start.elapsed();

        println!("{}", table.odb.total_mem_size_bytes());
        println!("{}", duration.as_millis());
    }

}


