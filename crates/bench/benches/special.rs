use criterion::{criterion_group, criterion_main, Criterion};
use mimalloc::MiMalloc;
use spacetimedb_bench::{
    database::BenchDatabase,
    schemas::{create_sequential, u32_u64_str, u32_u64_u64, u64_u64_u32, BenchTable, RandomTable},
    spacetime_module::{BenchModule, CSharp, Rust, SpacetimeModule},
};
use spacetimedb_lib::{bsatn::ToBsatn as _, ProductValue};
use spacetimedb_sats::sats::{self, bsatn};
use spacetimedb_schema::schema::TableSchema;
use std::time::Instant;
use std::{hint::black_box, sync::Arc};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn criterion_benchmark(c: &mut Criterion) {
    serialize_benchmarks::<u32_u64_str>(c);
    serialize_benchmarks::<u32_u64_u64>(c);
    serialize_benchmarks::<u64_u64_u32>(c);

    custom_module_benchmarks::<Rust>(c);
    custom_module_benchmarks::<CSharp>(c);
    custom_db_benchmarks::<Rust>(c);
    custom_db_benchmarks::<CSharp>(c);
}

fn custom_module_benchmarks<M: BenchModule>(c: &mut Criterion) {
    let db = SpacetimeModule::<M>::build(true, true).unwrap();

    let mut group = c.benchmark_group(format!("special/{}", M::BENCH_NAME));

    let args = sats::product!["0".repeat(65536).into_boxed_str()];
    group.bench_function("large_arguments/64KiB", |b| {
        b.iter_batched(
            || args.clone(),
            |args| {
                db.runtime
                    .block_on(async { db.module.call_reducer_binary("fn_with_1_args", args).await.unwrap() })
            },
            criterion::BatchSize::PerIteration,
        )
    });

    for n in [1u32, 100, 1000] {
        let args = sats::product![n];
        group.bench_function(&format!("print_bulk/lines={n}"), |b| {
            b.iter_batched(
                || args.clone(),
                |args| {
                    db.runtime
                        .block_on(async { db.module.call_reducer_binary("print_many_things", args).await.unwrap() })
                },
                criterion::BatchSize::PerIteration,
            )
        });
    }
}

fn custom_db_benchmarks<M: BenchModule>(c: &mut Criterion) {
    let db = SpacetimeModule::<M>::build(true, true).unwrap();

    let mut group = c.benchmark_group(format!("special/{}", M::BENCH_NAME));
    // This bench take long, so adjust for it
    // group.measurement_time(Duration::from_secs(60 * 2));
    group.sample_size(10);

    for n in [10, 100] {
        let args = sats::product![n];
        group.bench_function(&format!("db_game/circles/load={n}"), |b| {
            b.iter_custom(|iters| {
                db.runtime.block_on(async {
                    db.module
                        .call_reducer_binary("init_game_circles", args.clone())
                        .await
                        .unwrap()
                });

                let start = Instant::now();

                for _ in 0..iters {
                    black_box(db.runtime.block_on(async {
                        db.module
                            .call_reducer_binary("run_game_circles", args.clone())
                            .await
                            .ok()
                    }));
                }

                start.elapsed()
            })
        });
    }

    for n in [10, 100] {
        let args = sats::product![n];
        group.bench_function(&format!("db_game/ia_loop/load={n}"), |b| {
            b.iter_custom(|_iters| {
                db.runtime.block_on(async {
                    db.module
                        .call_reducer_binary("init_game_ia_loop", args.clone())
                        .await
                        .unwrap()
                });

                let start = Instant::now();

                black_box(db.runtime.block_on(async {
                    db.module
                        .call_reducer_binary("run_game_ia_loop", args.clone())
                        .await
                        .ok()
                }));

                start.elapsed()
            })
        });
    }
}

fn serialize_benchmarks<
    T: BenchTable + RandomTable + for<'a> spacetimedb_lib::de::Deserialize<'a> + for<'a> serde::de::Deserialize<'a>,
>(
    c: &mut Criterion,
) {
    let name = T::name();
    let count = 100;
    let mut group = c.benchmark_group(format!("special/serde/serialize/{name}"));
    group.throughput(criterion::Throughput::Elements(count));

    let data = create_sequential::<T>(0xdeadbeef, count as u32, 100);

    group.bench_function(&format!("product_value/count={count}"), |b| {
        b.iter_batched(
            || data.clone(),
            |data| data.into_iter().map(|row| row.into_product_value()).collect::<Vec<_>>(),
            criterion::BatchSize::PerIteration,
        );
    });
    // this measures serialization from a ProductValue, not directly (as in, from generated code in the Rust SDK.)
    let data_pv = data
        .into_iter()
        .map(|row| spacetimedb_lib::AlgebraicValue::Product(row.into_product_value()))
        .collect::<ProductValue>();

    group.bench_function(&format!("bsatn/count={count}"), |b| {
        b.iter_batched_ref(
            || data_pv.clone(),
            |data_pv| sats::bsatn::to_vec(data_pv).unwrap(),
            criterion::BatchSize::PerIteration,
        );
    });
    group.bench_function(&format!("json/count={count}"), |b| {
        b.iter_batched_ref(
            || data_pv.clone(),
            |data_pv| serde_json::to_string(data_pv).unwrap(),
            criterion::BatchSize::PerIteration,
        );
    });

    let mut table_schema = TableSchema::from_product_type(T::product_type());
    table_schema.table_name = name.into();
    let mut table = spacetimedb_table::table::Table::new(
        Arc::new(table_schema),
        spacetimedb_table::indexes::SquashedOffset::COMMITTED_STATE,
    );
    let mut blob_store = spacetimedb_table::blob_store::HashMapBlobStore::default();

    let ptrs = data_pv
        .elements
        .iter()
        .map(|row| {
            table
                .insert(&mut blob_store, row.as_product().unwrap())
                .unwrap()
                .1
                .pointer()
        })
        .collect::<Vec<_>>();
    let refs = ptrs
        .into_iter()
        .map(|ptr| table.get_row_ref(&blob_store, ptr).unwrap())
        .collect::<Vec<_>>();
    group.bench_function(&format!("bflatn_to_bsatn_slow_path/count={count}"), |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            for row_ref in &refs {
                bsatn::to_writer(&mut buf, row_ref).unwrap();
            }
            buf
        })
    });
    group.bench_function(&format!("bflatn_to_bsatn_fast_path/count={count}"), |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            for row_ref in &refs {
                row_ref.to_bsatn_extend(&mut buf).unwrap();
            }
            buf
        });
    });

    group.finish();

    let mut group = c.benchmark_group(format!("special/serde/deserialize/{name}"));
    group.throughput(criterion::Throughput::Elements(count));

    let data_bin = sats::bsatn::to_vec(&data_pv).unwrap();
    let data_json = serde_json::to_string(&data_pv).unwrap();

    group.bench_function(&format!("bsatn/count={count}"), |b| {
        b.iter_batched_ref(
            || data_bin.clone(),
            |data_bin| bsatn::from_slice::<Vec<T>>(data_bin).unwrap(),
            criterion::BatchSize::PerIteration,
        );
    });
    group.bench_function(&format!("json/count={count}"), |b| {
        b.iter_batched_ref(
            || data_json.clone(),
            |data_json| serde_json::from_str::<Vec<T>>(data_json).unwrap(),
            criterion::BatchSize::PerIteration,
        );
    });
    // TODO: deserialize benches (needs a typespace)
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
