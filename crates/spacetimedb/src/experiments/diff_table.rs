use std::collections::HashSet;
use sha3::digest::{generic_array::GenericArray, generic_array::typenum::U32};
use crate::{object_db::ObjectDB, hash::hash_bytes};

type Hash = GenericArray<u8, U32>;

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

#[derive(Debug, Copy, Clone)]
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
    writes: Vec<Write>,
}

pub struct Table {
    pub odb: ObjectDB,
    closed_state: HashSet<Hash>,
    closed_commit: Hash,
    closed_commit_offset: u64,
    open_commits: Vec<Hash>,
    branched_commits: Vec<Hash>,
}

impl Table {
    pub fn new() -> Self {
        let mut odb = ObjectDB::new();
        let initial_commit_bytes = Self::encode_commit(CommitObj {
            parent_commit_hash: None,
            writes: Vec::new(),
        });
        let commit_hash = odb.add(initial_commit_bytes);
        Self {
            odb,
            closed_state: HashSet::new(),
            closed_commit: commit_hash,
            closed_commit_offset: 0,
            branched_commits: Vec::new(),
            open_commits: Vec::new(),
        }
    }

    fn decode_commit(bytes: &[u8]) -> CommitObj {
        if bytes.len() == 0 {
            return CommitObj {
                parent_commit_hash: None,
                writes: Vec::new(),
            }
        }

        let start = 0;
        let end = 32;
        let mut parent_commit_hash = Hash::default();
        parent_commit_hash.copy_from_slice(&bytes[start..end]);

        let start = end;
        let end = start + 4;
        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[start..end]);
        let length = u32::from_le_bytes(dst);

        let mut writes: Vec<Write> = Vec::new();
        for i in 0..length {
            let start = (i * 33) as usize + end;
            let end = start + 33;
            let op = bytes[start];
            let mut hash = Hash::default();
            hash.copy_from_slice(&bytes[start+1..end]);
            if op == 0 {
                writes.push(Write::Delete(hash));
            } else {
                writes.push(Write::Insert(hash));
            }
        }
        CommitObj {
            parent_commit_hash: Some(parent_commit_hash),
            writes,
        }
    }

    fn decode_row(bytes: &[u8]) -> MyRowObj {
        unsafe { std::ptr::read(bytes.as_ptr() as *const _) }
    }
    
    fn encode_commit(commit: CommitObj) -> Vec<u8> {
        if commit.parent_commit_hash.is_none() {
            return Vec::new();
        }

        let mut commit_bytes = Vec::new();
        commit_bytes.reserve(32 + 4 + 1 + 32);
        if let Some(parent_commit_hash) = commit.parent_commit_hash {
            for byte in parent_commit_hash {
                commit_bytes.push(byte);
            }
        }

        commit_bytes.extend((commit.writes.len() as u32).to_le_bytes());

        for write in commit.writes {
            match write {
                Write::Insert(hash) => {
                    commit_bytes.push(1);
                    for byte in hash {
                        commit_bytes.push(byte);
                    }
                },
                Write::Delete(hash) => {
                    commit_bytes.push(0);
                    for byte in hash {
                        commit_bytes.push(byte);
                    }
                },
            }
        }

        return commit_bytes;
    }

    fn encode_row(row: MyRowObj) -> Vec<u8> {
        unsafe { any_as_u8_slice(&row) }.to_vec()
    }

    fn latest_commit(&self) -> Hash {
        self.open_commits.last().map(|h| *h).unwrap_or(self.closed_commit)
    }

    pub fn begin_tx(&mut self) -> Transaction {
        let parent = self.latest_commit();
        self.branched_commits.push(parent);
        Transaction {
            parent_commit: parent,
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

        // Search back through this transaction
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

        // Search backwards through all open commits that are parents of this transaction.
        // if you find a delete it's not there.
        // if you find an insert it is there. If you find no mention of it, then whether
        // it's there or not is dependent on the closed_state.
        let mut i = self.open_commits.iter().position(|h| *h == tx.parent_commit).unwrap_or(0);
        loop {
            let next_open = self.open_commits.get(i).map(|h| *h);
            if let Some(next_open) = next_open {
                let commit_obj = Self::decode_commit(self.odb.get(next_open).unwrap());
                for write in commit_obj.writes {
                    match write {
                        Write::Insert(h) => {
                            if h == hash {
                                return row_obj;
                            }
                        },
                        Write::Delete(h) => {
                            if h == hash {
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

        if self.closed_state.contains(&hash) {
            return Some(row_obj.unwrap());
        }

        None
    }

    pub fn scan(&self, _tx: &mut Transaction, _filter: fn(MyRowObj) -> bool) {
        // let latest_commit_obj = Self::decode_commit(self.odb.get(self.latest_commit()).unwrap());

        // let mut rows: HashSet<Hash> = self.closed_state;
        // for i in 0..tx.writes.len() {
        //     match &tx.writes[i] {
        //         Write::Insert(hash) => rows.insert(*hash),
        //         Write::Delete(hash) => rows.remove(hash),
        //     };
        // }

        // for hash in rows {
        //     let row_obj = Self::decode_row(self.odb.get(hash).unwrap());
        //     filter(row_obj);
        //     tx.reads.push(hash);
        // }
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

    fn finalize(&mut self, tx: Transaction) {
        // Rebase on the last open commit (or closed commit if none open)
        let new_commit = CommitObj {
            parent_commit_hash: Some(self.latest_commit()),
            writes: tx.writes,
        };

        let commit_bytes = Self::encode_commit(new_commit);
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
                        self.closed_commit_offset += 1;
                        self.open_commits.remove(0);

                        let commit_obj = Self::decode_commit(self.odb.get(next_open).unwrap());
                        for write in commit_obj.writes {
                            match write {
                                Write::Insert(hash) => {
                                    self.closed_state.insert(hash);
                                },
                                Write::Delete(hash) => {
                                    self.closed_state.remove(&hash);
                                },
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
        let mut read_set: HashSet<Hash> = HashSet::new();
        for read in &tx.reads {
            read_set.insert(*read);
        }

        let mut commit_hash = self.latest_commit();
        loop {
            let commit_obj = Self::decode_commit(self.odb.get(commit_hash).unwrap());
            for write in commit_obj.writes {
                match write {
                    Write::Insert(hash) => {
                        if read_set.contains(&hash) {
                            return false;
                        }
                    },
                    Write::Delete(hash) => {
                        if read_set.contains(&hash) {
                            return false;
                        }
                    },
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


