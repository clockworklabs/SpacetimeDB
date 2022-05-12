use crate::hash::hash_bytes;
use sha3::digest::{generic_array::typenum::U32, generic_array::GenericArray};
use std::collections::{HashMap, HashSet};

use super::object_db::ObjectDB;

// TODO: maybe use serde?
type Hash = GenericArray<u8, U32>;

#[derive(Debug, Clone)]
pub struct Transaction {
    parent_commit: Hash,
    reads: Vec<Read>,
    writes: Vec<Write>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct Read {
    set_id: u32,
    hash: Hash,
}

#[derive(Debug, Copy, Clone)]
enum Write {
    Insert { set_id: u32, hash: Hash },
    Delete { set_id: u32, hash: Hash },
}

impl Write {
    // write: <write_type(1)><hash(32)>
    fn decode(bytes: &mut &[u8]) -> Self {
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
            Write::Delete { set_id, hash }
        } else {
            Write::Insert { set_id, hash }
        }
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
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
            };
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
    closed_state: HashMap<u32, HashSet<Hash>>,
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
            closed_state: HashMap::new(),
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

    pub fn rollback_tx(&mut self, _tx: Transaction) {
        // TODO: clean up branched_commits
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
                let (set_id, hash) = match write {
                    Write::Insert { set_id, hash } => (set_id, hash),
                    Write::Delete { set_id, hash } => (set_id, hash),
                };
                if read_set.contains(&Read { set_id, hash }) {
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
        let index = self
            .branched_commits
            .iter()
            .position(|hash| *hash == tx.parent_commit)
            .unwrap();
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
                                Write::Insert { set_id, hash } => {
                                    if let Some(closed_set) = self.closed_state.get_mut(&set_id) {
                                        closed_set.insert(hash);
                                    } else {
                                        let mut hash_set = HashSet::new();
                                        hash_set.insert(hash);
                                        self.closed_state.insert(set_id, hash_set);
                                    }
                                }
                                Write::Delete { set_id, hash } => {
                                    if let Some(closed_set) = self.closed_state.get_mut(&set_id) {
                                        closed_set.remove(&hash);

                                        if closed_set.len() == 0 {
                                            drop(closed_set);
                                            self.closed_state.remove(&set_id);
                                        }
                                    } else {
                                        // Do nothing
                                    }
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

    pub fn seek(&self, tx: &mut Transaction, set_id: u32, hash: Hash) -> Option<&[u8]> {
        // I'm not sure if this just needs to track reads from the parent commit
        // or reads from the transaction as well.
        tx.reads.push(Read { set_id, hash });

        // Even uncommitted rows will be in the odb. This will accumulate garbage over time,
        // but we could also clear it if a commit fails (or store uncommited changes in a different odb).
        // You could potentially check if this is None to short circuit things, but that's
        // only if you're sure everything is in the odb.
        let row_obj = self.odb.get(hash);

        // Search back through this transaction
        for i in (0..tx.writes.len()).rev() {
            match &tx.writes[i] {
                Write::Insert { set_id: s, hash: h } => {
                    if *h == hash && *s == set_id {
                        return Some(row_obj.unwrap());
                    }
                }
                Write::Delete { set_id: s, hash: h } => {
                    if *h == hash && *s == set_id {
                        return None;
                    }
                }
            };
        }

        // Search backwards through all open commits that are parents of this transaction.
        // if you find a delete it's not there.
        // if you find an insert it is there. If you find no mention of it, then whether
        // it's there or not is dependent on the closed_state.
        let mut i = self
            .open_commits
            .iter()
            .position(|h| *h == tx.parent_commit)
            .unwrap_or(0);
        loop {
            let next_open = self.open_commits.get(i).map(|h| *h);
            if let Some(next_open) = next_open {
                let commit_obj = Commit::decode(&mut self.odb.get(next_open).unwrap());
                for write in &commit_obj.writes {
                    match write {
                        Write::Insert { set_id: s, hash: h } => {
                            if *h == hash && *s == set_id {
                                return row_obj;
                            }
                        }
                        Write::Delete { set_id: s, hash: h } => {
                            if *h == hash && *s == set_id {
                                return None;
                            }
                        }
                    }
                }
            } else {
                // No more commits to process
                break;
            }
            i -= 1;
        }

        if let Some(closed_set) = self.closed_state.get(&set_id) {
            if closed_set.contains(&hash) {
                return Some(row_obj.unwrap());
            }
        }

        None
    }

    pub fn scan<'a>(&'a self, tx: &'a mut Transaction, set_id: u32) -> ScanIter<'a> {
        let tx_writes_index = tx.writes.len() as i32 - 1;
        ScanIter {
            txdb: self,
            tx,
            set_id,
            scanned: HashSet::new(),
            scan_stage: Some(ScanStage::CurTx { index: tx_writes_index }),
        }
    }

    pub fn delete(&mut self, tx: &mut Transaction, set_id: u32, hash: Hash) {
        // Search backwards in the transaction:
        // if not there: add delete
        // if insert there: replace with delete
        // if delete there: do nothing
        let mut found = false;
        for i in (0..tx.writes.len()).rev() {
            let write = tx.writes[i];
            match write {
                Write::Insert { set_id: s, hash: h } => {
                    if h == hash && s == set_id {
                        found = true;
                        tx.writes[i] = Write::Delete { set_id, hash };
                        break;
                    }
                }
                Write::Delete { set_id: s, hash: h } => {
                    if h == hash && s == set_id {
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found {
            tx.writes.push(Write::Delete { set_id, hash });
        }
    }

    pub fn insert(&mut self, tx: &mut Transaction, set_id: u32, bytes: Vec<u8>) -> Hash {
        let hash = hash_bytes(&bytes);

        // Search backwards in the transaction:
        // if not there: add insert
        // if insert there: do nothing
        // if delete there: overwrite as insert
        let mut found = false;
        for i in (0..tx.writes.len()).rev() {
            let write = tx.writes[i];
            match write {
                Write::Insert { set_id: s, hash: h } => {
                    if h == hash && s == set_id {
                        found = true;
                        break;
                    }
                }
                Write::Delete { set_id: s, hash: h } => {
                    if h == hash && s == set_id {
                        found = true;
                        tx.writes[i] = Write::Insert { set_id, hash };
                        break;
                    }
                }
            }
        }
        if !found {
            // Add bytes to the odb
            self.odb.add(bytes);
            tx.writes.push(Write::Insert { set_id, hash });
        }

        hash
    }
}

pub struct ScanIter<'a> {
    txdb: &'a TransactionalDB,
    tx: &'a mut Transaction,
    set_id: u32,
    scanned: HashSet<Hash>,
    scan_stage: Option<ScanStage<'a>>,
}

enum ScanStage<'a> {
    CurTx { index: i32 },
    OpenCommits { commit: Commit, write_index: Option<usize> },
    ClosedSet(std::collections::hash_set::Iter<'a, Hash>),
}

impl<'a> Iterator for ScanIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let scanned = &mut self.scanned;
        let set_id = self.set_id;

        loop {
            match self.scan_stage.take() {
                Some(ScanStage::CurTx { index }) => {
                    // Search back through this transaction
                    if index == -1 {
                        let open_commits_index =
                            self.txdb.open_commits.iter().position(|h| *h == self.tx.parent_commit);
                        if let Some(open_commits_index) = open_commits_index {
                            let open_commit = self.txdb.open_commits[open_commits_index];
                            let commit = Commit::decode(&mut self.txdb.odb.get(open_commit).unwrap());
                            let index = if commit.writes.len() > 0 {
                                Some(commit.writes.len() - 1)
                            } else {
                                None
                            };
                            self.scan_stage = Some(ScanStage::OpenCommits {
                                commit,
                                write_index: index,
                            });
                        } else {
                            if let Some(closed_set) = self.txdb.closed_state.get(&set_id) {
                                self.scan_stage = Some(ScanStage::ClosedSet(closed_set.iter()));
                            } else {
                                return None;
                            }
                        }
                        continue;
                    } else {
                        self.scan_stage = Some(ScanStage::CurTx { index: index - 1 });
                    }
                    match &self.tx.writes[index as usize] {
                        Write::Insert { set_id: s, hash: h } => {
                            if *s == set_id {
                                self.tx.reads.push(Read { set_id: *s, hash: *h });
                                let row_obj = self.txdb.odb.get(*h).unwrap();
                                scanned.insert(*h);
                                return Some(row_obj);
                            }
                        }
                        Write::Delete { set_id: s, hash: h } => {
                            if *s == set_id {
                                self.tx.reads.push(Read { set_id: *s, hash: *h });
                                scanned.insert(*h);
                            }
                        }
                    };
                }
                Some(ScanStage::OpenCommits { commit, write_index }) => {
                    // Search backwards through all open commits that are parents of this transaction.
                    // if you find a delete it's not there.
                    // if you find an insert it is there. If you find no mention of it, then whether
                    // it's there or not is dependent on the closed_state.
                    let parent = commit.parent_commit_hash;
                    let mut opt_index = write_index;
                    loop {
                        if let Some(index) = opt_index {
                            let write = &commit.writes[index];
                            opt_index = if index > 0 { Some(index - 1) } else { None };
                            match write {
                                Write::Insert { set_id: s, hash: h } => {
                                    if *s == set_id && !scanned.contains(h) {
                                        self.tx.reads.push(Read { set_id: *s, hash: *h });
                                        let row_obj = self.txdb.odb.get(*h).unwrap();
                                        scanned.insert(*h);
                                        self.scan_stage = Some(ScanStage::OpenCommits {
                                            commit,
                                            write_index: opt_index,
                                        });
                                        return Some(row_obj);
                                    }
                                }
                                Write::Delete { set_id: s, hash: h } => {
                                    if *s == set_id {
                                        self.tx.reads.push(Read { set_id: *s, hash: *h });
                                        scanned.insert(*h);
                                    }
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    // TODO: can be optimized by storing the index
                    let open_commits_index =
                        parent.and_then(|parent| self.txdb.open_commits.iter().position(|h| *h == parent));
                    if let Some(open_commits_index) = open_commits_index {
                        let open_commit = self.txdb.open_commits[open_commits_index];
                        let commit = Commit::decode(&mut self.txdb.odb.get(open_commit).unwrap());
                        let index = if commit.writes.len() > 0 {
                            Some(commit.writes.len() - 1)
                        } else {
                            None
                        };
                        self.scan_stage = Some(ScanStage::OpenCommits {
                            commit,
                            write_index: index,
                        });
                    } else {
                        if let Some(closed_set) = self.txdb.closed_state.get(&set_id) {
                            self.scan_stage = Some(ScanStage::ClosedSet(closed_set.iter()));
                        } else {
                            return None;
                        }
                    }
                    continue;
                }
                Some(ScanStage::ClosedSet(mut closed_set_iter)) => {
                    let h = closed_set_iter.next();
                    if let Some(h) = h {
                        self.scan_stage = Some(ScanStage::ClosedSet(closed_set_iter));
                        if !scanned.contains(h) {
                            self.tx.reads.push(Read { set_id, hash: *h });
                            let row_obj = self.txdb.odb.get(*h).unwrap();
                            return Some(row_obj);
                        }
                    } else {
                        return None;
                    }
                }
                None => {
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TransactionalDB;
    use crate::hash::hash_bytes;
    use crate::hash::Hash;

    unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
        ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
    }

    #[repr(C, packed)]
    #[derive(Debug, Copy, Clone)]
    pub struct MyStruct {
        my_name: Hash,
        my_i32: i32,
        my_u64: u64,
        my_hash: Hash,
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
        let row_key_1 = db.insert(&mut tx, 0, b"this is a byte string".to_vec());
        db.commit_tx(tx);

        let mut tx = db.begin_tx();
        let row = db.seek(&mut tx, 0, row_key_1).unwrap();

        assert_eq!(b"this is a byte string", row);
    }

    #[test]
    fn test_insert_and_seek_struct() {
        let mut db = TransactionalDB::new();
        let mut tx = db.begin_tx();
        let row_key_1 = db.insert(
            &mut tx,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        db.commit_tx(tx);

        let mut tx = db.begin_tx();
        let row = MyStruct::decode(db.seek(&mut tx, 0, row_key_1).unwrap());

        let i = row.my_i32;
        assert_eq!(i, -1);
    }

    #[test]
    fn test_read_isolation() {
        let mut db = TransactionalDB::new();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, 0, row_key_1);
        assert!(row.is_none());

        db.commit_tx(tx_1);

        let mut tx_3 = db.begin_tx();
        let row = db.seek(&mut tx_3, 0, row_key_1);
        assert!(row.is_some());
    }

    #[test]
    fn test_scan() {
        let mut db = TransactionalDB::new();
        let mut tx_1 = db.begin_tx();
        let _row_key_1 = db.insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        let _row_key_2 = db.insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -2,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );

        let mut scan_1 = db.scan(&mut tx_1, 0).map(|b| b.to_owned()).collect::<Vec<Vec<u8>>>();
        scan_1.sort();

        db.commit_tx(tx_1);

        let mut tx_2 = db.begin_tx();
        let mut scan_2 = db.scan(&mut tx_2, 0).collect::<Vec<&[u8]>>();
        scan_2.sort();

        assert_eq!(scan_1.len(), scan_2.len());

        for (i, _) in scan_1.iter().enumerate() {
            let val_1 = &scan_1[i];
            let val_2 = scan_2[i];
            for i in 0..val_1.len() {
                assert_eq!(val_1[i], val_2[i]);
            }
        }
    }

    #[test]
    fn test_write_skew_conflict() {
        let mut db = TransactionalDB::new();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, 0, row_key_1);
        assert!(row.is_none());

        assert!(db.commit_tx(tx_1));
        assert!(!db.commit_tx(tx_2));
    }

    #[test]
    fn test_write_skew_no_conflict() {
        let mut db = TransactionalDB::new();
        let mut tx_1 = db.begin_tx();
        let row_key_1 = db.insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -1,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        let row_key_2 = db.insert(
            &mut tx_1,
            0,
            MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -2,
                my_u64: 1,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode(),
        );
        assert!(db.commit_tx(tx_1));

        let mut tx_2 = db.begin_tx();
        let row = db.seek(&mut tx_2, 0, row_key_1);
        assert!(row.is_some());
        db.delete(&mut tx_2, 0, row_key_2);

        let mut tx_3 = db.begin_tx();
        db.delete(&mut tx_3, 0, row_key_1);

        assert!(db.commit_tx(tx_2));
        assert!(db.commit_tx(tx_3));
    }

    #[test]
    fn test_size() {
        let start = std::time::Instant::now();
        let mut db = TransactionalDB::new();
        let iterations: u128 = 1000;
        println!("{} odb base size bytes", db.odb.total_mem_size_bytes());

        let mut raw_data_size = 0;
        for i in 0..iterations {
            let mut tx_1 = db.begin_tx();
            let val_1 = MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -(i as i32),
                my_u64: i as u64,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode();
            let val_2 = MyStruct {
                my_name: hash_bytes(b"This is a byte string."),
                my_i32: -2 * (i as i32),
                my_u64: i as u64,
                my_hash: hash_bytes(b"This will be turned into a hash."),
            }
            .encode();

            raw_data_size += val_1.len() as u64;
            raw_data_size += val_2.len() as u64;

            db.insert(&mut tx_1, 0, val_1);
            db.insert(&mut tx_1, 0, val_2);

            assert!(db.commit_tx(tx_1));
        }
        let duration = start.elapsed();
        println!("{} odb after size bytes", db.odb.total_mem_size_bytes());

        // each key is this long: "qwertyuiopasdfghjklzxcvbnm123456";
        // key x2: 64 bytes
        // commit key: 32 bytes
        // commit: 98 bytes <parent(32)><write(<type(1)><hash(32)>)><write(<type(1)><hash(32)>)>
        // total: 194
        let data_overhead = db.odb.total_mem_size_bytes() - raw_data_size;
        println!("{} overhead bytes per tx", data_overhead / iterations as u64);
        println!("{} us per tx", duration.as_micros() / iterations);
    }
}
