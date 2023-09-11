//! Benchmarks for evaluating how we fare against sqlite

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use spacetimedb_bench::prelude::*;
use std::time::Duration;

// IMPORTANT!: It needs this option to run the setup once per `.iter`!
const SIZE: BatchSize = BatchSize::PerIteration;

fn build_group<'a>(c: &'a mut Criterion, named: &str, run: Runs) -> BenchmarkGroup<'a, WallTime> {
    let mut group = c.benchmark_group(named);

    group.throughput(Throughput::Elements(run as u64));
    group.sample_size(DB_POOL as usize);
    group.measurement_time(Duration::from_secs(5));

    group
}

fn bench_insert_tx_per_row(c: &mut Criterion, fsync: bool) {
    let run = Runs::Tiny;
    let mut group = build_group(c, "insert_row", run);
    let parameter = if fsync { "fsync" } else { "no-fsync" };

    // Factor out the db creation because is IO that generate noise
    group.bench_function(BenchmarkId::new(SQLITE, parameter), |b| {
        b.iter_batched(
            || sqlite::create_db(0, true).unwrap(),
            |path| {
                let mut conn = sqlite::open_conn(&path, fsync).unwrap();
                sqlite::insert_tx_per_row(&mut conn, run).unwrap();
            },
            SIZE,
        );
    });
    group.bench_function(BenchmarkId::new(SPACETIME, parameter), |b| {
        b.iter_batched(
            || spacetime::create_db(0, true).unwrap(),
            |path| {
                let (conn, _, table_id) = spacetime::open_conn(&path, fsync).unwrap();
                spacetime::insert_tx_per_row(&conn, table_id, run).unwrap();
            },
            SIZE,
        );
    });

    group.finish();
}

fn bench_insert_tx(c: &mut Criterion, fsync: bool) {
    let run = Runs::Small;
    let mut group = build_group(c, "insert_bulk_rows", run);
    let parameter = if fsync { "fsync" } else { "no-fsync" };

    // Factor out the db creation because is IO that generate noise
    group.bench_function(BenchmarkId::new(SQLITE, parameter), |b| {
        b.iter_batched(
            || sqlite::create_db(0, true).unwrap(),
            |path| {
                let mut conn = sqlite::open_conn(&path, fsync).unwrap();
                sqlite::insert_tx(&mut conn, run).unwrap();
            },
            SIZE,
        );
    });
    group.bench_function(BenchmarkId::new(SPACETIME, parameter), |b| {
        b.iter_batched(
            || spacetime::create_db(0, true).unwrap(),
            |path| {
                let (conn, _, table_id) = spacetime::open_conn(&path, fsync).unwrap();
                spacetime::insert_tx(&conn, table_id, run).unwrap();
            },
            SIZE,
        );
    });

    group.finish();
}

fn bench_select_no_index(c: &mut Criterion, fsync: bool) {
    let run = Runs::Tiny;
    let mut group = build_group(c, "select_index_no", run);
    let parameter = if fsync { "fsync" } else { "no-fsync" };

    // Factor out the db creation because is IO that generate noise
    group.bench_function(BenchmarkId::new(SQLITE, parameter), |b| {
        b.iter_batched(
            || {
                let path = sqlite::create_db(0, true).unwrap();
                sqlite::db_prefill(&path, run, true).unwrap();
                path
            },
            |path| {
                let mut conn = sqlite::open_conn(&path, fsync).unwrap();
                sqlite::select_no_index(&mut conn, run).unwrap();
            },
            SIZE,
        );
    });
    group.bench_function(BenchmarkId::new(SPACETIME, parameter), |b| {
        b.iter_batched(
            || {
                let path = spacetime::create_db(0, true).unwrap();
                spacetime::db_prefill(&path, run, true).unwrap();
                path
            },
            |path| {
                let (conn, _, table_id) = spacetime::open_conn(&path, fsync).unwrap();
                spacetime::select_no_index(&conn, table_id, run).unwrap();
            },
            SIZE,
        );
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
