use criterion::{criterion_group, criterion_main, Bencher, BenchmarkId, Criterion, Throughput, BenchmarkGroup};
use rand::Rng;
use spacetimedb::db::ostorage::hashmap_object_db::HashMapObjectDB;
use spacetimedb::db::ostorage::ObjectDB;
use std::collections::VecDeque;
use criterion::measurement::WallTime;
use tempdir::TempDir;

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

// Spam the DB continuous with inserting new values.
fn add<F>(bench: &mut Bencher, valgen: F)
    where
        F: Fn() -> Vec<u8>,
{
    let tmp_dir = TempDir::new("odb_bench").unwrap();
    let mut db = HashMapObjectDB::open(tmp_dir.path()).unwrap();
    bench.iter(move || {
        let bytes = valgen();
        db.add(bytes);
    });
}

// Add one value then retrieve it over and over to measure (cached) retrieval times.
fn get<F>(bench: &mut Bencher, valgen: F)
    where
        F: Fn() -> Vec<u8>,
{
    let tmp_dir = TempDir::new("odb_bench").unwrap();
    let mut db = HashMapObjectDB::open(tmp_dir.path()).unwrap();
    let bytes = valgen();
    let hash = db.add(bytes);
    bench.iter(move || {
        db.get(hash);
    });
}

// Add new values and then retrieve them immediately over and over again
fn add_get<F>(bench: &mut Bencher, valgen: F)
    where
        F: Fn() -> Vec<u8>,
{
    let tmp_dir = TempDir::new("odb_bench").unwrap();
    let mut db = HashMapObjectDB::open(tmp_dir.path()).unwrap();
    bench.iter(move || {
        let bytes = valgen();
        let hash = db.add(bytes);
        db.get(hash);
    });
}

// Add new values but retrieve them later instead of immediately after inserting.
fn add_get_delayed<F>(bench: &mut Bencher, valgen: F, delay_count: usize)
    where
        F: Fn() -> Vec<u8>,
{
    let tmp_dir = TempDir::new("odb_bench").unwrap();
    let mut db = HashMapObjectDB::open(tmp_dir.path()).unwrap();

    // Keep N items in our hash stack, pushing new hashes to the end and popping old ones off
    // the front
    let mut hash_stack = VecDeque::new();

    for _i in 0..delay_count {
        let bytes = valgen();
        hash_stack.push_back(db.add(bytes));
    }

    bench.iter(move || {
        let bytes = valgen();
        let new_hash = db.add(bytes);
        hash_stack.push_back(new_hash);
        let old_hash = hash_stack.pop_front().unwrap();
        db.get(old_hash);
    });
}


fn perform_bench<F>(bench_group: &mut BenchmarkGroup<WallTime>, valgen: &F)
    where
        F: Fn() -> Vec<u8>,
{
    bench_group.bench_function("add", |bench| add(bench, valgen));
    bench_group.bench_function("get", |bench| get(bench, valgen));
    bench_group.bench_function("add_get", |bench| add_get(bench, valgen));
    for delay_count in [4, 8, 64] {
        bench_group.bench_with_input(
            BenchmarkId::new("add_get_delayed", delay_count),
            &delay_count,
            |bench, &delay_count| add_get_delayed(bench, valgen, delay_count),
        );
    }
}

fn latency_bench(c: &mut Criterion) {
    let mut latency_bench_group = c.benchmark_group("object_db_latency");
    let latency_valgen = || generate_random_sized_value();

    perform_bench(&mut latency_bench_group, &latency_valgen);

    latency_bench_group.finish();
}

fn throughput_bench(c: &mut Criterion) {
    let mut throughput_bench_group = c.benchmark_group("object_db_throughput");
    throughput_bench_group.throughput(Throughput::Bytes(THROUGHPUT_BENCH_VALUE_SIZE as u64));
    let throughput_valgen = || generate_value(THROUGHPUT_BENCH_VALUE_SIZE);

    perform_bench(&mut throughput_bench_group, &throughput_valgen);

    throughput_bench_group.finish();
}

criterion_group!(benches, latency_bench, throughput_bench);
criterion_main!(benches);
