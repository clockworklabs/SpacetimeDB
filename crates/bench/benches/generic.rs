use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    Bencher, BenchmarkGroup, Criterion,
};
use lazy_static::lazy_static;
use spacetimedb_bench::{
    database::BenchDatabase,
    schemas::{create_sequential, u32_u64_str, u32_u64_u64, BenchTable, IndexStrategy, RandomTable},
    spacetime_module, spacetime_raw, sqlite, ResultBench,
};
use spacetimedb_lib::sats::AlgebraicType;
use spacetimedb_primitives::ColId;
use spacetimedb_testing::modules::{Csharp, Rust};

#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

lazy_static! {
    static ref RUN_ONE_MILLION: bool = std::env::var("RUN_ONE_MILLION").is_ok();
}

fn criterion_benchmark(c: &mut Criterion) {
    bench_suite::<sqlite::SQLite>(c, true).unwrap();
    bench_suite::<spacetime_raw::SpacetimeRaw>(c, true).unwrap();
    bench_suite::<spacetime_module::SpacetimeModule<Rust>>(c, true).unwrap();
    bench_suite::<spacetime_module::SpacetimeModule<Csharp>>(c, true).unwrap();

    bench_suite::<sqlite::SQLite>(c, false).unwrap();
    bench_suite::<spacetime_raw::SpacetimeRaw>(c, false).unwrap();
    bench_suite::<spacetime_module::SpacetimeModule<Rust>>(c, false).unwrap();
    bench_suite::<spacetime_module::SpacetimeModule<Csharp>>(c, false).unwrap();
}

#[inline(never)]
fn bench_suite<DB: BenchDatabase>(c: &mut Criterion, in_memory: bool) -> ResultBench<()> {
    let mut db = DB::build(in_memory)?;
    let param_db_name = DB::name();
    let param_in_memory = if in_memory { "mem" } else { "disk" };
    let db_params = format!("{param_db_name}/{param_in_memory}");

    let mut g = c.benchmark_group(&db_params);

    empty(&mut g, &mut db)?;

    table_suite::<DB, u32_u64_str>(&mut g, &mut db)?;
    table_suite::<DB, u32_u64_u64>(&mut g, &mut db)?;

    Ok(())
}

type Group<'a> = BenchmarkGroup<'a, WallTime>;

#[inline(never)]
fn table_suite<DB: BenchDatabase, T: BenchTable + RandomTable>(g: &mut Group, db: &mut DB) -> ResultBench<()> {
    // This setup is a compromise between trying to present related benchmarks together,
    // and not having to deal with nasty reentrant generic dispatching.

    type TableData<TableId> = (IndexStrategy, TableId, String);
    let mut prep_table = |index_strategy: IndexStrategy| -> ResultBench<TableData<DB::TableId>> {
        let table_name = T::name();
        let style_name = index_strategy.name();
        let table_params = format!("{table_name}/{style_name}");
        let table_id = db.create_table::<T>(index_strategy)?;

        Ok((index_strategy, table_id, table_params))
    };
    let tables: [TableData<DB::TableId>; 2] = [
        prep_table(IndexStrategy::Unique0)?,
        //prep_table(IndexStrategy::NoIndex)?,
        prep_table(IndexStrategy::BTreeEachColumn)?,
    ];

    for (_, table_id, table_params) in &tables {
        insert_bulk::<DB, T>(g, table_params, db, table_id, 2048, 256)?;
        if *RUN_ONE_MILLION {
            insert_bulk::<DB, T>(g, table_params, db, table_id, 0, 1_000_000)?;
        }
    }
    for (index_strategy, table_id, table_params) in &tables {
        if *index_strategy == IndexStrategy::Unique0 {
            // Iterate is unaffected by index strategy, so only run it here
            iterate::<DB, T>(g, table_params, db, table_id, 256)?;
            // Update can only be performed with a unique key
            update_bulk::<DB, T>(g, table_params, db, table_id, 2048, 256)?;
            if *RUN_ONE_MILLION {
                update_bulk::<DB, T>(g, table_params, db, table_id, 1_000_000, 1_000_000)?;
            }
        } else {
            // perform "filter" benchmarks
            filter::<DB, T>(g, db, table_id, index_strategy, 2, 2048, 8)?;
        }
    }

    Ok(())
}

/// Custom criterion timing loop. Allows access to a database, which is reset every benchmark iteration.
///
/// The prepare closure should restore the database to known state
/// and prepare any inputs consumed by the benchmark. The time this takes will
/// not be measured.
///
/// You should clear all modified tables after calling this, just to be safe.
#[inline(never)]
fn bench_harness<
    DB: BenchDatabase,
    INPUT,
    PREPARE: FnMut(&mut DB) -> ResultBench<INPUT>,
    ROUTINE: FnMut(&mut DB, INPUT) -> ResultBench<()>,
>(
    b: &mut Bencher,
    db: &mut DB,
    mut prepare: PREPARE,
    mut routine: ROUTINE,
) {
    b.iter_custom(|n| {
        let timer = WallTime;
        let mut elapsed = timer.zero();

        for _ in 0..n {
            let input = prepare(db).unwrap();

            // only nanoseconds of overhead
            let start = timer.start();
            routine(db, input).unwrap();
            let just_elapsed = timer.end(start);

            elapsed = timer.add(&elapsed, &just_elapsed);
        }
        elapsed
    });
}

#[inline(never)]
fn empty<DB: BenchDatabase>(g: &mut Group, db: &mut DB) -> ResultBench<()> {
    let id = "empty".to_string();
    g.bench_function(&id, |b| {
        bench_harness(
            b,
            db,
            |_| Ok(()),
            |db, _| {
                // not much to do in this one
                db.empty_transaction()
            },
        )
    });
    Ok(())
}

#[inline(never)]
fn insert_bulk<DB: BenchDatabase, T: BenchTable + RandomTable>(
    g: &mut Group,
    table_params: &str,
    db: &mut DB,
    table_id: &DB::TableId,
    load: u32,
    count: u32,
) -> ResultBench<()> {
    let id = format!("insert_bulk/{table_params}/load={load}/count={count}");
    let data = create_sequential::<T>(0xdeadbeef, load + count, 1000);

    // Each iteration performs one transaction, though it inserts many rows.
    g.throughput(criterion::Throughput::Elements(1));
    // FIXME: only for 1_000_000 inserts
    g.sample_size(10);

    g.bench_function(&id, |b| {
        bench_harness(
            b,
            db,
            |db| {
                let mut data = data.clone();
                db.clear_table(table_id)?;
                let to_insert = data.split_off(load as usize);
                if !data.is_empty() {
                    db.insert_bulk(table_id, data)?;
                }
                Ok(to_insert)
            },
            |db, to_insert| {
                db.insert_bulk(table_id, to_insert)?;
                Ok(())
            },
        )
    });
    db.clear_table(table_id)?;
    Ok(())
}

#[inline(never)]
fn update_bulk<DB: BenchDatabase, T: BenchTable + RandomTable>(
    g: &mut Group,
    table_params: &str,
    db: &mut DB,
    table_id: &DB::TableId,
    load: u32,
    count: u32,
) -> ResultBench<()> {
    let id = format!("update_bulk/{table_params}/load={load}/count={count}");
    let data = create_sequential::<T>(0xdeadbeef, load, 1000);

    // Each iteration performs one transaction, though it inserts many rows.
    g.throughput(criterion::Throughput::Elements(1));

    // running a big guy
    g.sample_size(10);

    g.bench_function(&id, |b| {
        bench_harness(
            b,
            db,
            |db| {
                let data = data.clone();
                db.clear_table(table_id)?;
                db.insert_bulk(table_id, data)?;
                Ok(())
            },
            |db, _| {
                db.update_bulk::<T>(table_id, count)?;
                Ok(())
            },
        )
    });
    db.clear_table(table_id)?;
    Ok(())
}

#[inline(never)]
fn iterate<DB: BenchDatabase, T: BenchTable + RandomTable>(
    g: &mut Group,
    table_params: &str,
    db: &mut DB,
    table_id: &DB::TableId,
    count: u32,
) -> ResultBench<()> {
    let id = format!("iterate/{table_params}/count={count}");
    let data = create_sequential::<T>(0xdeadbeef, count, 1000);

    db.insert_bulk(table_id, data)?;

    // Each iteration performs a single transaction,
    // though it iterates across many rows.
    g.throughput(criterion::Throughput::Elements(1));

    g.bench_function(&id, |b| {
        bench_harness(
            b,
            db,
            |_| Ok(()),
            |db, _| {
                db.iterate(table_id)?;
                Ok(())
            },
        )
    });
    db.clear_table(table_id)?;
    Ok(())
}

#[inline(never)]
fn filter<DB: BenchDatabase, T: BenchTable + RandomTable>(
    g: &mut Group,
    db: &mut DB,
    table_id: &DB::TableId,
    index_strategy: &IndexStrategy,
    col_id: impl Into<ColId>,
    load: u32,
    buckets: u32,
) -> ResultBench<()> {
    let col_id = col_id.into();

    let filter_column_type = match T::product_type().elements[col_id.idx()].algebraic_type {
        AlgebraicType::String => "string",
        AlgebraicType::U32 => "u32",
        AlgebraicType::U64 => "u64",
        _ => unimplemented!(),
    };
    let mean_result_count = load / buckets;
    let indexed = match index_strategy {
        IndexStrategy::BTreeEachColumn => "index",
        IndexStrategy::NoIndex => "no_index",
        _ => unimplemented!(),
    };
    let id = format!("filter/{filter_column_type}/{indexed}/load={load}/count={mean_result_count}");

    let data = create_sequential::<T>(0xdeadbeef, load, buckets as u64);

    db.insert_bulk(table_id, data.clone())?;

    // Each iteration performs a single transaction.
    g.throughput(criterion::Throughput::Elements(1));

    // We loop through all buckets found in the sample data.
    // This mildly increases variance on the benchmark, but makes "mean_result_count" more accurate.
    // Note that all databases have EXACTLY the same sample data.
    let mut i = 0;

    g.bench_function(&id, |b| {
        bench_harness(
            b,
            db,
            |_| {
                // pick something to look for
                let value = data[i].clone().into_product_value().elements[col_id.idx()].clone();
                i = (i + 1) % load as usize;
                Ok(value)
            },
            |db, value| {
                db.filter::<T>(table_id, col_id, value)?;
                Ok(())
            },
        )
    });
    db.clear_table(table_id)?;
    Ok(())
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
