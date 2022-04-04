use std::collections::HashSet;
use sha3::digest::{generic_array::GenericArray, generic_array::typenum::U32};
use crate::{hash::hash_bytes, object_db::ObjectDB};
// TODO: maybe use serde?

struct Namespace {
    name: String,

}

type Hash = GenericArray<u8, U32>;
pub struct Transaction {
    parent_commit: Hash,
    reads: Vec<Read>,
    writes: Vec<Write>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct Read {
    hash: Hash,
}

#[derive(Debug, Copy, Clone)]
enum Write {
    Insert(Hash),
    Delete(Hash),
}

impl Write {
    // write: <write_type(1)><hash(32)>
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
struct Commit {
    parent_commit_hash: Option<Hash>,
    writes: Vec<Write>,
}

impl Commit {
    // commit: <parent_commit_hash(32)>[<table_update>...(dedupped and sorted_numerically)]*
    fn decode(bytes: &mut &[u8]) -> Self {
        if bytes.len() == 0 {
            return Commit {
                parent_commit_hash: None,
                writes: Vec::new(),
            }
        }

        let start = 0;
        let end = 32;
        let mut parent_commit_hash = Hash::default();
        parent_commit_hash.copy_from_slice(&bytes[start..end]);

        *bytes = &bytes[end..];
        let mut writes: Vec<Write> = Vec::new();
        while bytes.len() > 0 {
            writes.push(Write::decode(bytes));
        }

        Commit {
            parent_commit_hash: Some(parent_commit_hash),
            writes,
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

        for update in &self.writes {
            update.encode(bytes);
        }
    }

}

// TODO: implement some kind of tag/dataset/namespace system
// which allows the user to restrict the search for data
// to a tag or set of tags. Honestly, maybe forget the content
// addressing and just use a map like CockroachDB, idk.
pub struct TransactionalDB {
    pub odb: ObjectDB,
    closed_state: HashSet<Hash>,
    closed_commit: Hash,
    open_commits: Vec<Hash>,
    branched_commits: Vec<Hash>,
}

impl TransactionalDB {
    pub fn new() -> Self {
        let commit = Commit {
            parent_commit_hash: None,
            writes: Vec::new(),
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
            closed_state: HashSet::new(),
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
            reads: Vec::new(),
            writes: Vec::new(),
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
        let mut read_set: HashSet<Read> = HashSet::new();
        for read in &tx.reads {
            read_set.insert(*read);
        }

        let mut commit_hash = self.latest_commit();
        loop {
            let commit = Commit::decode(&mut self.odb.get(commit_hash).unwrap());
            for write in commit.writes {
                let hash = match write {
                    Write::Insert(hash) => hash,
                    Write::Delete(hash) => hash,
                };
                if read_set.contains(&Read { hash }) {
                    return false;
                }
            }

            if commit.parent_commit_hash == Some(tx.parent_commit) {
                break;
            }

            commit_hash = commit.parent_commit_hash.unwrap();
        }

        self.finalize(tx);
        true
    }

    fn finalize(&mut self, tx: Transaction) {
        // Rebase on the last open commit (or closed commit if none open)
        let new_commit = Commit {
            parent_commit_hash: Some(self.latest_commit()),
            writes: tx.writes,
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

                        let commit_obj = Commit::decode(&mut self.odb.get(next_open).unwrap());
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

    pub fn seek(&self, tx: &mut Transaction, hash: Hash) -> Option<&[u8]> {
        // I'm not sure if this just needs to track reads from the parent commit
        // or reads from the transaction as well.
        tx.reads.push(Read { hash });

        // Even uncommitted rows will be in the odb. This will accumulate garbage over time,
        // but we could also clear it if a commit fails (or store uncommited changes in a different odb).
        // You could potentially check if this is None to short circuit things, but that's
        // only if you're sure everything is in the odb.
        let row_obj = self.odb.get(hash);

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
                let commit_obj = Commit::decode(&mut self.odb.get(next_open).unwrap());
                for write in &commit_obj.writes {
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

        if self.closed_state.contains(&hash) {
            return Some(row_obj.unwrap());
        }

        None
    }

    pub fn scan<F>(&mut self, tx: &mut Transaction, mut callback: F)
    where
        Self: Sized,
        F: FnMut(&[u8]),
    {
        let mut scanned: HashSet<Hash> = HashSet::new();

        // Search back through this transaction
        for i in (0..tx.writes.len()).rev() {
            match &tx.writes[i] {
                Write::Insert(h) => {
                    tx.reads.push(Read { hash: *h });
                    let row_obj = self.odb.get(*h).unwrap();
                    scanned.insert(*h);
                    callback(row_obj);
                },
                Write::Delete(h) => {
                    tx.reads.push(Read { hash: *h });
                    scanned.insert(*h);
                }
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
                let commit_obj = Commit::decode(&mut self.odb.get(next_open).unwrap());
                for write in &commit_obj.writes {
                    match write {
                        Write::Insert(h) => {
                            if !scanned.contains(h) {
                                tx.reads.push(Read { hash: *h });
                                let row_obj = self.odb.get(*h).unwrap();
                                scanned.insert(*h);
                                callback(row_obj);
                            }
                        },
                        Write::Delete(h) => {
                            tx.reads.push(Read { hash: *h });
                            scanned.insert(*h);
                        },
                    }
                }
            } else {
                // No more commits to process
                break;
            }
            i -= 1;
        }

        for h in &self.closed_state {
            if !scanned.contains(h) {
                tx.reads.push(Read { hash: *h });
                let row_obj = self.odb.get(*h).unwrap();
                callback(row_obj);
            }
        }
    }

    pub fn delete(&mut self, tx: &mut Transaction, hash: Hash) {
        // Search backwards in the transaction:
        // if not there: add delete
        // if insert there: replace with delete
        // if delete there: do nothing
        let mut found = false;
        for i in (0..tx.writes.len()).rev() {
            let write = tx.writes[i];
            match write {
                Write::Insert(h) => {
                    if h == hash {
                        found = true;
                        tx.writes[i] = Write::Delete(hash);
                        break;
                    }
                },
                Write::Delete(h) => {
                    if h == hash {
                        found = true;
                        break;
                    }
                },
            }
        }
        if !found {
            tx.writes.push(Write::Delete(hash));
        }
    }

    pub fn insert(&mut self, tx: &mut Transaction, bytes: Vec<u8>) -> Hash {
        let hash = hash_bytes(&bytes);

        // Search backwards in the transaction:
        // if not there: add insert
        // if insert there: do nothing
        // if delete there: overwrite as insert
        let mut found = false;
        for i in (0..tx.writes.len()).rev() {
            let write = tx.writes[i];
            match write {
                Write::Insert(h) => {
                    if h == hash {
                        found = true;
                        break;
                    }
                },
                Write::Delete(h) => {
                    if h == hash {
                        found = true;
                        tx.writes[i] = Write::Insert(hash);
                        break;
                    }
                },
            }
        }
        if !found {
            // Add bytes to the odb
            self.odb.add(bytes);
            tx.writes.push(Write::Insert(hash));
        }

        hash
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::hash_bytes;
    use crate::hash::Hash;
    use super::TransactionalDB;

    unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
        ::std::slice::from_raw_parts(
            (p as *const T) as *const u8,
            ::std::mem::size_of::<T>(),
        )
    }
    
    #[repr(C, packed)]
    #[derive(Debug, Copy, Clone)]
    pub struct MyStruct {
        my_name: Hash,
        my_i32: i32,
        my_u64: u64,
        my_hash: Hash
    }

    impl MyStruct {
        fn encode(&self) -> Vec<u8> {
            unsafe { any_as_u8_slice(self) }.to_vec()
        }

        fn decode(bytes: &[u8]) -> Self {
            unsafe { std::ptr::read(bytes.as_ptr() as *const _) }
        }
    }

    #[test]
    fn test_insert_and_seek_bytes() {
        let mut db = TransactionalDB::new();
        let mut tx = db.begin_tx();
        let row_key_1 = db.insert(&mut tx, b"this is a byte string".to_vec());
        db.commit_tx(tx);

        let mut tx = db.begin_tx();
        let row = db.seek(&mut tx, row_key_1).unwrap();

        assert_eq!(b"this is a byte string", row);
    }

    #[test]
    fn test_insert_and_seek_struct() {
        let mut db = TransactionalDB::new();
        let mut tx = db.begin_tx();
        let row_key_1 = db.insert(&mut tx, MyStruct {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -1,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        }.encode());
        db.commit_tx(tx);

        let mut tx = db.begin_tx();
        let row = MyStruct::decode(db.seek(&mut tx, row_key_1).unwrap());

        let i = row.my_i32;
        assert_eq!(i, -1);
    }

    #[test]
    fn test_read_isolation() {
        let mut db = TransactionalDB::new();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(&mut tx_1, MyStruct {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -1,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        }.encode());

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, row_key_1);
        assert!(row.is_none());
        
        db.commit_tx(tx_1);
        
        let mut tx_3 = db.begin_tx();
        let row = db.seek(&mut tx_3, row_key_1);
        assert!(row.is_some());
    }

    #[test]
    fn test_write_skew_conflict() {
        let mut db = TransactionalDB::new();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(&mut tx_1, MyStruct {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -1,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        }.encode());

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, row_key_1);
        assert!(row.is_none());
        
        assert!(db.commit_tx(tx_1));
        assert!(!db.commit_tx(tx_2));
    }

    #[test]
    fn test_write_skew_no_conflict() {
        let mut db = TransactionalDB::new();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(&mut tx_1, MyStruct {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -1,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        }.encode());
        let row_key_2 = db.insert(&mut tx_1, MyStruct {
            my_name: hash_bytes(b"This is a byte string."),
            my_i32: -2,
            my_u64: 1,
            my_hash: hash_bytes(b"This will be turned into a hash."),
        }.encode());
        assert!(db.commit_tx(tx_1));

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, row_key_1);
        assert!(row.is_some());
        db.delete(&mut tx_2, row_key_2);
        
        let mut tx_3 = db.begin_tx();
        db.delete(&mut tx_3, row_key_1);
        
        assert!(db.commit_tx(tx_2));
        assert!(db.commit_tx(tx_3));
    }

    #[test]
    fn test_size() {
        let start = std::time::Instant::now();
        let mut db = TransactionalDB::new();
        let iterations: u128 = 1000;
        println!("{} odb base size bytes",  db.odb.total_mem_size_bytes());

        let mut raw_data_size = 0;
        for i in 0..iterations {
            let mut tx_1 = db.begin_tx();
            let val_1 = MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -(i as i32),
                my_u64: i as u64,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }.encode();
            let val_2 = MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -2 * (i as i32),
                my_u64: i as u64,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }.encode();

            raw_data_size += val_1.len() as u64;
            raw_data_size += val_2.len() as u64;

            db.insert(&mut tx_1, val_1);
            db.insert(&mut tx_1, val_2);

            assert!(db.commit_tx(tx_1));
        }
        let duration = start.elapsed();
        println!("{} odb after size bytes",  db.odb.total_mem_size_bytes());

        // each key is this long: "qwertyuiopasdfghjklzxcvbnm123456";
        // key x2: 64 bytes
        // commit key: 32 bytes
        // commit: 98 bytes <parent(32)><write(<type(1)><hash(32)>)><write(<type(1)><hash(32)>)>
        // total: 194
        let data_overhead = db.odb.total_mem_size_bytes() - raw_data_size;
        println!("{} overhead bytes per tx",  data_overhead / iterations as u64);
        println!("{} us per tx", duration.as_micros() / iterations);
    }

}


