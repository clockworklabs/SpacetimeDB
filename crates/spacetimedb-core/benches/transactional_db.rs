use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, Bencher, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use rand::Rng;
use spacetimedb::db::message_log::MessageLog;
use spacetimedb::db::ostorage::hashmap_object_db::HashMapObjectDB;

#[cfg(feature = "odb_rocksdb")]
use spacetimedb::db::ostorage::rocks_object_db::RocksDBObjectDB;

#[cfg(feature = "odb_sled")]
use spacetimedb::db::ostorage::sled_object_db::SledObjectDB;

use spacetimedb::db::ostorage::ObjectDB;
use spacetimedb::db::transactional_db::TransactionalDB;
use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tempdir::TempDir;

const VALUE_MAX_SIZE: usize = 4096;
const MAX_TEST_SETS: u32 = 32;
const THROUGHPUT_BENCH_VALUE_SIZE: usize = 1024;

// The different possible storage engine backend types.
#[derive(Clone, Copy)]
pub enum ODBFlavor {
    HashMap,
    #[cfg(feature = "odb_rocksdb")]
    Sled,
    #[cfg(feature = "odb_sled")]
    Rocks,
}
impl Display for ODBFlavor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ODBFlavor::HashMap => f.write_str("HashMap"),
            #[cfg(feature = "odb_sled")]
            ODBFlavor::Sled => f.write_str("Sled"),
            #[cfg(feature = "odb_rocksdb")]
            ODBFlavor::Rocks => f.write_str("Rocks"),
        }
    }
}

fn open_hm(root: &Path) -> Box<dyn ObjectDB + Send> {
    Box::new(HashMapObjectDB::open(root.to_path_buf().join("odb")).unwrap())
}
#[cfg(feature = "odb_sled")]
fn open_sled(root: &Path) -> Box<dyn ObjectDB + Send> {
    Box::new(SledObjectDB::open(root.to_path_buf().join("odb")).unwrap())
}
#[cfg(feature = "odb_rocksdb")]
fn open_rocks(root: &Path) -> Box<dyn ObjectDB + Send> {
    Box::new(RocksDBObjectDB::open(root.to_path_buf().join("odb")).unwrap())
}

fn open_db(flavor: ODBFlavor) -> fn(&Path) -> Box<dyn ObjectDB + Send> {
    match flavor {
        ODBFlavor::HashMap => open_hm,
        #[cfg(feature = "odb_sled")]
        ODBFlavor::Sled => open_sled,
        #[cfg(feature = "odb_rocksdb")]
        ODBFlavor::Rocks => open_rocks,
    }
}

fn generate_value(value_size: usize) -> (u32 /* set_id */, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let set_id = rng.gen_range(0..MAX_TEST_SETS);
    (set_id, (0..value_size).map(|_| rng.gen::<u8>()).collect())
}

fn generate_random_sized_value() -> (u32 /* set_id */, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let size: usize = rng.gen_range(0..VALUE_MAX_SIZE);
    let set_id = rng.gen_range(0..MAX_TEST_SETS);
    (set_id, (0..size).map(|_| rng.gen::<u8>()).collect())
}

// Spam the DB continuous with inserting new values.
fn insert_commit<F>(bench: &mut Bencher, flavor: ODBFlavor, valgen: F)
where
    F: Fn() -> (u32 /* set_id */, Vec<u8>),
{
    let tmp_dir = TempDir::new("txdb_bench").unwrap();
    let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog")).unwrap()));
    let odb = Arc::new(Mutex::new(open_db(flavor)(tmp_dir.path().join("odb").as_path())));
    let mut db = TransactionalDB::open(mlog, odb).unwrap();
    bench.iter(move || {
        let mut tx = db.begin_tx();
        let (set_id, bytes) = valgen();
        db.insert(&mut tx, set_id, bytes);
        assert!(db.commit_tx(tx).is_some());
    });
}

// Insert one set value then retrieve it over and over to measure (cached) retrieval times.
fn seek<F>(bench: &mut Bencher, flavor: ODBFlavor, valgen: F)
where
    F: Fn() -> (u32 /* set_id */, Vec<u8>),
{
    let tmp_dir = TempDir::new("txdb_bench").unwrap();
    let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog")).unwrap()));
    let odb = Arc::new(Mutex::new(open_db(flavor)(tmp_dir.path().join("odb").as_path())));
    let mut db = TransactionalDB::open(mlog, odb).unwrap();
    let mut tx = db.begin_tx();
    let (set_id, bytes) = valgen();
    let hash = db.insert(&mut tx, set_id, bytes);
    assert!(db.commit_tx(tx).is_some());
    bench.iter(move || {
        let mut tx = db.begin_tx();
        db.seek(&mut tx, 0, hash);
    });
}

// Add new values and then retrieve them immediately over and over again in the same tx.
fn insert_seek<F>(bench: &mut Bencher, flavor: ODBFlavor, valgen: F)
where
    F: Fn() -> (u32 /* set_id */, Vec<u8>),
{
    let tmp_dir = TempDir::new("txdb_bench").unwrap();
    let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog")).unwrap()));
    let odb = Arc::new(Mutex::new(open_db(flavor)(tmp_dir.path().join("odb").as_path())));
    let mut db = TransactionalDB::open(mlog, odb).unwrap();
    bench.iter(move || {
        let mut tx = db.begin_tx();
        let (set_id, bytes) = valgen();
        let datakey = db.insert(&mut tx, set_id, bytes);
        db.seek(&mut tx, set_id, datakey);
        db.commit_tx(tx);
    });
}

// Add new values and then retrieve them immediately over and over again, but in two separate
// transactions.
fn insert_seek_new_tx<F>(bench: &mut Bencher, flavor: ODBFlavor, valgen: F)
where
    F: Fn() -> (u32 /* set_id */, Vec<u8>),
{
    let tmp_dir = TempDir::new("txdb_bench").unwrap();
    let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog")).unwrap()));
    let odb = Arc::new(Mutex::new(open_db(flavor)(tmp_dir.path().join("odb").as_path())));
    let mut db = TransactionalDB::open(mlog, odb).unwrap();
    bench.iter(move || {
        let (set_id, data_key) = {
            let mut tx = db.begin_tx();
            let (set_id, bytes) = valgen();
            let dk = db.insert(&mut tx, set_id, bytes);
            db.commit_tx(tx);
            (set_id, dk)
        };
        {
            let mut tx = db.begin_tx();
            db.seek(&mut tx, set_id, data_key);
        }
    });
}

// Add new values but retrieve them later instead of immediately after inserting.
fn insert_seek_delayed<F>(bench: &mut Bencher, valgen: F, flavor: ODBFlavor, delay_count: usize)
where
    F: Fn() -> (u32 /* set_id */, Vec<u8>),
{
    let tmp_dir = TempDir::new("txdb_bench").unwrap();
    let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog")).unwrap()));
    let odb = Arc::new(Mutex::new(open_db(flavor)(tmp_dir.path().join("odb").as_path())));
    let mut db = TransactionalDB::open(mlog, odb).unwrap();

    // Keep N items in our hash stack, pushing new hashes to the end and popping old ones off
    // the front
    let mut datakey_stack = VecDeque::new();

    for _i in 0..delay_count {
        let mut tx = db.begin_tx();
        let (set_id, bytes) = valgen();
        datakey_stack.push_back((set_id, db.insert(&mut tx, set_id, bytes)));
        db.commit_tx(tx);
    }

    bench.iter(move || {
        let mut tx = db.begin_tx();

        let (set_id, bytes) = valgen();
        let new_datakey = db.insert(&mut tx, set_id, bytes);
        datakey_stack.push_back((set_id, new_datakey));
        let (set_id, old_datakey) = datakey_stack.pop_front().unwrap();
        db.seek(&mut tx, set_id, old_datakey);
        db.commit_tx(tx);
    });
}

#[derive(Clone, Copy)]
struct FlavoredDelayedCount {
    count: usize,
    flavor: ODBFlavor,
}
impl Display for FlavoredDelayedCount {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}", self.flavor, self.count)
    }
}

fn perform_bench<F>(bench_group: &mut BenchmarkGroup<WallTime>, valgen: &F, flavor: ODBFlavor)
where
    F: Fn() -> (u32 /* set_id */, Vec<u8>),
{
    bench_group.bench_with_input(BenchmarkId::new("insert_commit", &flavor), &flavor, |bench, &flavor| {
        insert_commit(bench, flavor, valgen)
    });
    bench_group.bench_with_input(BenchmarkId::new("seek", &flavor), &flavor, |bench, &flavor| {
        seek(bench, flavor, valgen)
    });
    bench_group.bench_with_input(BenchmarkId::new("insert_seek", &flavor), &flavor, |bench, &flavor| {
        insert_seek(bench, flavor, valgen)
    });
    bench_group.bench_with_input(
        BenchmarkId::new("insert_seek_sep_tx", &flavor),
        &flavor,
        |bench, &flavor| insert_seek_new_tx(bench, flavor, valgen),
    );

    for delay_count in [16, 64] {
        let param = FlavoredDelayedCount {
            count: delay_count,
            flavor: flavor.clone(),
        };
        bench_group.bench_with_input(
            BenchmarkId::new("insert_seek_delayed", param),
            &param,
            |bench, &param| insert_seek_delayed(bench, valgen, param.flavor, param.count),
        );
    }
}

fn latency_bench(c: &mut Criterion) {
    let mut latency_bench_group = c.benchmark_group("transactional_db_latency");
    let latency_valgen = || generate_random_sized_value();

    perform_bench(&mut latency_bench_group, &latency_valgen, ODBFlavor::HashMap);
    #[cfg(feature = "odb_sled")]
    perform_bench(&mut latency_bench_group, &latency_valgen, ODBFlavor::Sled);
    #[cfg(feature = "odb_rocksdb")]
    perform_bench(&mut latency_bench_group, &latency_valgen, ODBFlavor::Rocks);

    latency_bench_group.finish();
}

fn throughput_bench(c: &mut Criterion) {
    // Tests DB throughput in various scenarios, using fixed size values.
    let mut throughput_bench_group = c.benchmark_group("transactional_db_throughput");
    throughput_bench_group.throughput(Throughput::Bytes(THROUGHPUT_BENCH_VALUE_SIZE as u64));
    let throughput_valgen = || generate_value(THROUGHPUT_BENCH_VALUE_SIZE);

    perform_bench(&mut throughput_bench_group, &throughput_valgen, ODBFlavor::HashMap);
    #[cfg(feature = "odb_sled")]
    perform_bench(&mut throughput_bench_group, &throughput_valgen, ODBFlavor::Sled);
    #[cfg(feature = "odb_rocksdb")]
    perform_bench(&mut throughput_bench_group, &throughput_valgen, ODBFlavor::Rocks);

    throughput_bench_group.finish();
}

criterion_group!(benches, latency_bench, throughput_bench);
criterion_main!(benches);
