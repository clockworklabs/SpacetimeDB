//! Benchmarks for evaluating how we fare against sqlite

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, Throughput};
use spacetimedb_bench::prelude::*;

fn build_group<'a>(c: &'a mut Criterion, named: &str, run: Runs) -> BenchmarkGroup<'a, WallTime> {
    let mut group = c.benchmark_group(named);
    group.throughput(Throughput::Elements(run as u64));
    group.sample_size(DB_POOL as usize);
    group.sampling_mode(SamplingMode::Linear);
    group
}

fn bench_insert_tx_per_row(c: &mut Criterion, fsync: bool) {
    let run = Runs::Tiny;
    let mut group = build_group(c, "insert_row", run);
    let parameter = if fsync { "fsync" } else { "no-fsync" };

    group.bench_function(BenchmarkId::new(SQLITE, parameter), |b| {
        let mut pool = Pool::new(false, fsync).unwrap();
        b.iter(|| sqlite::insert_tx_per_row(&mut pool, run).unwrap())
    });
    group.bench_function(BenchmarkId::new(SPACETIME, parameter), |b| {
        let mut pool = Pool::new(false, fsync).unwrap();
        b.iter(|| spacetime::insert_tx_per_row(&mut pool, run).unwrap())
    });

    group.finish();
}

fn bench_insert_tx(c: &mut Criterion, fsync: bool) {
    let run = Runs::Small;
    let mut group = build_group(c, "insert_bulk_rows", run);
    let parameter = if fsync { "fsync" } else { "no-fsync" };

    group.bench_function(BenchmarkId::new(SQLITE, parameter), |b| {
        let mut pool = Pool::new(true, fsync).unwrap();
        b.iter(|| sqlite::insert_tx(&mut pool, run))
    });
    group.bench_function(BenchmarkId::new(SPACETIME, parameter), |b| {
        let mut pool = Pool::new(true, fsync).unwrap();
        b.iter(|| spacetime::insert_tx(&mut pool, run))
    });

    group.finish();
}

fn bench_select_no_index(c: &mut Criterion, fsync: bool) {
    let run = Runs::Tiny;
    let mut group = build_group(c, "select_index_no", run);
    let parameter = if fsync { "fsync" } else { "no-fsync" };

    group.bench_function(BenchmarkId::new(SQLITE, parameter), |b| {
        let mut pool = Pool::new(true, fsync).unwrap();
        b.iter(|| sqlite::select_no_index(&mut pool, run).unwrap())
    });
    group.bench_function(BenchmarkId::new(SPACETIME, parameter), |b| {
        let mut pool = Pool::new(true, fsync).unwrap();
        b.iter(|| spacetime::select_no_index(&mut pool, run).unwrap())
    });

    group.finish();
}

fn bench_insert_tx_per_row_with_fsync(c: &mut Criterion) {
    bench_insert_tx_per_row(c, true);
}

fn bench_insert_tx_per_row_no_fsync(c: &mut Criterion) {
    bench_insert_tx_per_row(c, false);
}

fn bench_insert_tx_with_fsync(c: &mut Criterion) {
    bench_insert_tx(c, true);
}

fn bench_insert_tx_no_fsync(c: &mut Criterion) {
    bench_insert_tx(c, false);
}

fn bench_select_no_index_with_fsync(c: &mut Criterion) {
    bench_select_no_index(c, true);
}

fn bench_select_no_index_no_fsync(c: &mut Criterion) {
    bench_select_no_index(c, false);
}

// Note: Reflex this same benchmarks in `main.rs`
criterion_group!(
    benches,
    bench_insert_tx_per_row_with_fsync,
    bench_insert_tx_per_row_no_fsync,
    bench_insert_tx_with_fsync,
    bench_insert_tx_no_fsync,
    bench_select_no_index_with_fsync,
    bench_select_no_index_no_fsync
);
criterion_main!(benches);
