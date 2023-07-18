//! Benchmarks for evaluating how we fare against sqlite

use criterion::measurement::WallTime;
use criterion::{
    criterion_group, criterion_main, BatchSize, BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use spacetimedb_bench::prelude::*;
use std::time::Duration;

fn build_group<'a>(c: &'a mut Criterion, named: &str, run: Runs) -> BenchmarkGroup<'a, WallTime> {
    let mut group = c.benchmark_group(named);
    // We need to restrict the amount of iterations and set the benchmark for "large" operations.
    group.throughput(Throughput::Elements(run as u64));
    group.sample_size(DB_POOL as usize);
    group.sampling_mode(SamplingMode::Flat);
    group
}

fn bench_insert_tx_per_row(c: &mut Criterion) {
    let run = Runs::Tiny;
    let mut group = build_group(c, "insert_row", run);

    group.bench_function(BenchmarkId::new(SQLITE, 1), |b| {
        let mut db_instance = 0;
        b.iter_batched(
            || {
                let path = sqlite::create_db(db_instance).unwrap();
                db_instance += 1;
                path
            },
            |data| {
                let mut conn = sqlite::open_conn(&data).unwrap();
                sqlite::insert_tx_per_row(&mut conn, run).unwrap();
            },
            BatchSize::NumBatches(DB_POOL as u64),
        );
    });
    // group.bench_function(BenchmarkId::new(SPACETIME, 1), |b| {
    //     let mut pool = Pool::new(false).unwrap();
    //     b.iter_with_setup(|| spacetime::insert_tx_per_row(&mut pool, run).unwrap())
    // });

    group.finish();
}
//
// fn bench_insert_tx(c: &mut Criterion) {
//     let run = Runs::Small;
//     let mut group = build_group(c, "insert_bulk_rows", run);
//
//     group.bench_function(BenchmarkId::new(SQLITE, 2), |b| {
//         let mut pool = Pool::new(true).unwrap();
//         b.iter(|| sqlite::insert_tx(&mut pool, run))
//     });
//     group.bench_function(BenchmarkId::new(SPACETIME, 2), |b| {
//         let mut pool = Pool::new(true).unwrap();
//         b.iter(|| spacetime::insert_tx(&mut pool, run))
//     });
//
//     group.finish();
// }
//
// fn bench_select_no_index(c: &mut Criterion) {
//     let run = Runs::Tiny;
//     let mut group = build_group(c, "select_index_no", run);
//
//     group.bench_function(BenchmarkId::new(SQLITE, 3), |b| {
//         let mut pool = Pool::new(true).unwrap();
//         b.iter(|| sqlite::select_no_index(&mut pool, run).unwrap())
//     });
//     group.bench_function(BenchmarkId::new(SPACETIME, 3), |b| {
//         let mut pool = Pool::new(true).unwrap();
//         b.iter(|| spacetime::select_no_index(&mut pool, run).unwrap())
//     });
//
//     group.finish();
// }

// Note: Reflex this same benchmarks in `main.rs`
//, bench_insert_tx, bench_select_no_index
criterion_group!(benches, bench_insert_tx_per_row);
criterion_main!(benches);
