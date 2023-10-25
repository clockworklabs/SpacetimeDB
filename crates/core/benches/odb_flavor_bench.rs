//! Benchmarks for evaluating various ODB storage systems for blob storage.

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, Bencher, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use rand::Rng;
use spacetimedb::db::ostorage::hashmap_object_db::HashMapObjectDB;

#[cfg(feature = "odb_rocksdb")]
use spacetimedb::db::ostorage::rocks_object_db::RocksDBObjectDB;

#[cfg(feature = "odb_sled")]
use spacetimedb::db::ostorage::sled_object_db::SledObjectDB;

use spacetimedb::db::ostorage::ObjectDB;
use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::path::Path;
use tempfile::TempDir;

const VALUE_MAX_SIZE: usize = 4096;
const THROUGHPUT_BENCH_VALUE_SIZE: usize = 1024;

fn generate_value(value_size: usize) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    (0..value_size).map(|_| rng.gen::<u8>()).collect()
}

fn generate_random_sized_value() -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let size: usize = rng.gen_range(0..VALUE_MAX_SIZE);
    (0..size).map(|_| rng.gen::<u8>()).collect()
}

// The different possible storage engine backend types.
#[derive(Clone, Copy)]
pub enum ODBFlavor {
    HashMap,
    #[cfg(feature = "odb_sled")]
    Sled,
    #[cfg(feature = "odb_rocksdb")]
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

fn open_db(root: &Path, flavor: ODBFlavor) -> Result<Box<dyn ObjectDB + Send>, anyhow::Error> {
    let odb: Box<dyn ObjectDB + Send> = match flavor {
        ODBFlavor::HashMap => Box::new(HashMapObjectDB::open(root.to_path_buf().join("odb"))?),
        #[cfg(feature = "odb_sled")]
        ODBFlavor::Sled => Box::new(SledObjectDB::open(root.to_path_buf().join("odb"))?),
        #[cfg(feature = "odb_rocksdb")]
        ODBFlavor::Rocks => Box::new(RocksDBObjectDB::open(root.to_path_buf().join("odb"))?),
    };
    Ok(odb)
}

// Spam the DB continuous with inserting new values.
fn add<F>(bench: &mut Bencher, flavor: ODBFlavor, valgen: F)
where
    F: Fn() -> Vec<u8>,
{
    let tmp_dir = TempDir::with_prefix("txdb_bench").unwrap();
    let mut db = open_db(tmp_dir.path(), flavor).unwrap();
    bench.iter_with_setup(valgen, move |bytes| {
        db.add(bytes);
    });
}

// Add one value then retrieve it over and over to measure (cached) retrieval times.
fn get<F>(bench: &mut Bencher, flavor: ODBFlavor, valgen: F)
where
    F: Fn() -> Vec<u8>,
{
    let tmp_dir = TempDir::with_prefix("odb_bench").unwrap();
    let mut db = open_db(tmp_dir.path(), flavor).unwrap();
    let bytes = valgen();
    let hash = db.add(bytes.clone());
    bench.iter(move || {
        let result = db.get(hash);
        assert_eq!(result.unwrap(), bytes.to_vec());
    });
}

// Add new values and then retrieve them immediately over and over again
fn add_get<F>(bench: &mut Bencher, flavor: ODBFlavor, valgen: F)
where
    F: Fn() -> Vec<u8>,
{
    let tmp_dir = TempDir::with_prefix("odb_bench").unwrap();
    let mut db = open_db(tmp_dir.path(), flavor).unwrap();
    bench.iter_with_setup(valgen, move |bytes| {
        let hash = db.add(bytes.clone());
        let result = db.get(hash);
        assert_eq!(result.unwrap(), bytes.to_vec());
    });
}

// Add new values but retrieve them later instead of immediately after inserting.
fn add_get_delayed<F>(bench: &mut Bencher, flavor: ODBFlavor, valgen: F, delay_count: usize)
where
    F: Fn() -> Vec<u8>,
{
    let tmp_dir = TempDir::with_prefix("txdb_bench").unwrap();
    let mut db = open_db(tmp_dir.path(), flavor).unwrap();

    // Keep N items in our hash stack, pushing new hashes to the end and popping old ones off
    // the front
    let mut hash_stack = VecDeque::new();

    for _i in 0..delay_count {
        let bytes = valgen();
        hash_stack.push_back(db.add(bytes));
    }

    bench.iter_with_setup(valgen, move |bytes| {
        let new_hash = db.add(bytes);
        hash_stack.push_back(new_hash);
        let old_hash = hash_stack.pop_front().unwrap();
        db.get(old_hash);
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
    F: Fn() -> Vec<u8>,
{
    bench_group.bench_with_input(BenchmarkId::new("add", flavor), &flavor, |bench, &flavor| {
        add(bench, flavor, valgen)
    });
    bench_group.bench_with_input(BenchmarkId::new("get", flavor), &flavor, |bench, &flavor| {
        get(bench, flavor, valgen)
    });
    bench_group.bench_with_input(BenchmarkId::new("add_get", flavor), &flavor, |bench, &flavor| {
        add_get(bench, flavor, valgen)
    });
    for delay_count in [16, 64] {
        let param = FlavoredDelayedCount {
            count: delay_count,
            flavor,
        };
        bench_group.bench_with_input(BenchmarkId::new("add_get_delayed", param), &param, |bench, &param| {
            add_get_delayed(bench, param.flavor, valgen, param.count)
        });
    }
}

fn latency_bench(c: &mut Criterion) {
    let mut latency_bench_group = c.benchmark_group("object_db_latency");
    let latency_valgen = generate_random_sized_value;

    perform_bench(&mut latency_bench_group, &latency_valgen, ODBFlavor::HashMap);
    #[cfg(feature = "odb_sled")]
    perform_bench(&mut latency_bench_group, &latency_valgen, ODBFlavor::Sled);
    #[cfg(feature = "odb_rocksdb")]
    perform_bench(&mut latency_bench_group, &latency_valgen, ODBFlavor::Rocks);

    latency_bench_group.finish();
}

fn throughput_bench(c: &mut Criterion) {
    let mut throughput_bench_group = c.benchmark_group("object_db_throughput");
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
