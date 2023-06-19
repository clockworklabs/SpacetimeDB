//! Benchmarks for evaluating speed of raw datastores.

use std::fmt::{Display, Formatter};

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, Bencher, BenchmarkGroup, BenchmarkId, Criterion, Throughput};

use rand::Rng;
use spacetimedb::db::datastore::gitlike_tx_blobstore::Gitlike;
use spacetimedb::db::datastore::traits::{MutTx, MutTxBlobstore, TableId};
use std::collections::VecDeque;

const VALUE_MAX_SIZE: usize = 4096;
const MAX_TEST_SETS: u32 = 32;
const THROUGHPUT_BENCH_VALUE_SIZE: usize = 1024;

fn open_datastore(flavor: DatastoreFlavor) -> impl MutTxBlobstore<TableId = TableId> {
    match flavor {
        DatastoreFlavor::Gitlike => Gitlike::open_blobstore(),
        DatastoreFlavor::Hekaton => unimplemented!(),
    }
}

fn generate_value(value_size: usize) -> (TableId, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let set_id = rng.gen_range(0..MAX_TEST_SETS);
    (
        TableId::from_u32_for_testing(set_id),
        (0..value_size).map(|_| rng.gen::<u8>()).collect(),
    )
}

fn generate_random_sized_value() -> (TableId, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let size: usize = rng.gen_range(0..VALUE_MAX_SIZE);
    let set_id = rng.gen_range(0..MAX_TEST_SETS);
    (
        TableId::from_u32_for_testing(set_id),
        (0..size).map(|_| rng.gen::<u8>()).collect(),
    )
}

// Spam the DB continuous with inserting new values.
fn insert_commit<F>(bench: &mut Bencher, flavor: DatastoreFlavor, valgen: F)
where
    F: Fn() -> (TableId, Vec<u8>),
{
    let db = open_datastore(flavor);
    bench.iter_with_setup(valgen, |(set_id, bytes)| {
        let mut tx = db.begin_mut_tx();
        db.insert_row_blob_mut_tx(&mut tx, set_id, &bytes).unwrap();
        assert!(db.commit_mut_tx(tx).unwrap().is_some());
    });
}

// Insert one set value then retrieve it over and over to measure (cached) retrieval times.
fn seek<F>(bench: &mut Bencher, flavor: DatastoreFlavor, valgen: F)
where
    F: Fn() -> (TableId, Vec<u8>),
{
    let db = open_datastore(flavor);
    let mut tx = db.begin_mut_tx();
    let (set_id, bytes) = valgen();
    let hash = db.insert_row_blob_mut_tx(&mut tx, set_id, &bytes).unwrap();
    assert!(db.commit_mut_tx(tx).unwrap().is_some());
    bench.iter(move || {
        let tx = db.begin_mut_tx();
        db.get_row_blob_mut_tx(&tx, TableId::from_u32_for_testing(0), hash)
            .unwrap();
    });
}

// Add new values and then retrieve them immediately over and over again in the same tx.
fn insert_seek<F>(bench: &mut Bencher, flavor: DatastoreFlavor, valgen: F)
where
    F: Fn() -> (TableId, Vec<u8>),
{
    let db = open_datastore(flavor);
    bench.iter_with_setup(valgen, |(set_id, bytes)| {
        let mut tx = db.begin_mut_tx();
        let datakey = db.insert_row_blob_mut_tx(&mut tx, set_id, &bytes).unwrap();
        db.get_row_blob_mut_tx(&tx, set_id, datakey).unwrap();
        db.commit_mut_tx(tx).unwrap();
    });
}

// Add new values and then retrieve them immediately over and over again, but in two separate
// transactions.
fn insert_seek_new_tx<F>(bench: &mut Bencher, flavor: DatastoreFlavor, valgen: F)
where
    F: Fn() -> (TableId, Vec<u8>),
{
    let db = open_datastore(flavor);
    bench.iter_with_setup(valgen, |(set_id, bytes)| {
        let data_key = {
            let mut tx = db.begin_mut_tx();
            let dk = db.insert_row_blob_mut_tx(&mut tx, set_id, &bytes).unwrap();
            db.commit_mut_tx(tx).unwrap();
            dk
        };
        {
            let tx = db.begin_mut_tx();
            db.get_row_blob_mut_tx(&tx, set_id, data_key).unwrap();
        }
    });
}

// Add new values but retrieve them later instead of immediately after inserting.
fn insert_seek_delayed<F>(bench: &mut Bencher, valgen: F, flavor: DatastoreFlavor, delay_count: usize)
where
    F: Fn() -> (TableId, Vec<u8>),
{
    let db = open_datastore(flavor);

    // Keep N items in our hash stack, pushing new hashes to the end and popping old ones off
    // the front
    let mut datakey_stack = VecDeque::new();

    for _i in 0..delay_count {
        let mut tx = db.begin_mut_tx();
        let (set_id, bytes) = valgen();
        datakey_stack.push_back((set_id, db.insert_row_blob_mut_tx(&mut tx, set_id, &bytes).unwrap()));
        db.commit_mut_tx(tx).unwrap();
    }

    bench.iter_with_setup(valgen, |(set_id, bytes)| {
        let mut tx = db.begin_mut_tx();

        let new_datakey = db.insert_row_blob_mut_tx(&mut tx, set_id, &bytes).unwrap();
        datakey_stack.push_back((set_id, new_datakey));
        let (set_id, old_datakey) = datakey_stack.pop_front().unwrap();
        db.get_row_blob_mut_tx(&tx, set_id, old_datakey).unwrap();
        db.commit_mut_tx(tx).unwrap();
    });
}

#[derive(Clone, Copy)]
pub enum DatastoreFlavor {
    Gitlike,
    Hekaton,
}

impl Display for DatastoreFlavor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gitlike => f.write_str("Gitlike"),
            Self::Hekaton => f.write_str("Hekaton"),
        }
    }
}

#[derive(Clone, Copy)]
struct FlavoredDelayedCount {
    count: usize,
    flavor: DatastoreFlavor,
}

impl Display for FlavoredDelayedCount {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}", self.flavor, self.count)
    }
}

fn perform_bench<F>(bench_group: &mut BenchmarkGroup<WallTime>, valgen: &F, flavor: DatastoreFlavor)
where
    F: Fn() -> (TableId, Vec<u8>),
{
    bench_group.bench_with_input(BenchmarkId::new("insert_commit", flavor), &flavor, |bench, &flavor| {
        insert_commit(bench, flavor, valgen)
    });
    bench_group.bench_with_input(BenchmarkId::new("seek", flavor), &flavor, |bench, &flavor| {
        seek(bench, flavor, valgen)
    });
    bench_group.bench_with_input(BenchmarkId::new("insert_seek", flavor), &flavor, |bench, &flavor| {
        insert_seek(bench, flavor, valgen)
    });
    bench_group.bench_with_input(
        BenchmarkId::new("insert_seek_sep_tx", flavor),
        &flavor,
        |bench, &flavor| insert_seek_new_tx(bench, flavor, valgen),
    );

    for delay_count in [16, 64] {
        let param = FlavoredDelayedCount {
            count: delay_count,
            flavor,
        };
        bench_group.bench_with_input(
            BenchmarkId::new("insert_seek_delayed", param),
            &param,
            |bench, &param| insert_seek_delayed(bench, valgen, param.flavor, param.count),
        );
    }
}

fn latency_bench(c: &mut Criterion) {
    let mut latency_bench_group = c.benchmark_group("tx_datastore_latency");
    let latency_valgen = generate_random_sized_value;

    perform_bench(&mut latency_bench_group, &latency_valgen, DatastoreFlavor::Gitlike);
    // perform_bench(&mut latency_bench_group, &latency_valgen, DatastoreFlavor::Hekaton);

    latency_bench_group.finish();
}

fn throughput_bench(c: &mut Criterion) {
    // Tests DB throughput in various scenarios, using fixed size values.
    let mut throughput_bench_group = c.benchmark_group("tx_datastore_throughput");
    throughput_bench_group.throughput(Throughput::Bytes(THROUGHPUT_BENCH_VALUE_SIZE as u64));
    let throughput_valgen = || generate_value(THROUGHPUT_BENCH_VALUE_SIZE);

    perform_bench(
        &mut throughput_bench_group,
        &throughput_valgen,
        DatastoreFlavor::Gitlike,
    );
    // perform_bench(&mut throughput_bench_group, &throughput_valgen, DatastoreFlavor::Hekaton);

    throughput_bench_group.finish();
}

criterion_group!(benches, latency_bench, throughput_bench);
criterion_main!(benches);
