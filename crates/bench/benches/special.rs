use criterion::async_executor::AsyncExecutor;
use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use spacetimedb_bench::{
    database::BenchDatabase,
    schemas::{create_sequential, u32_u64_str, u32_u64_u64, u64_u64_u32, BenchTable, RandomTable},
    spacetime_module::SpacetimeModule,
};
use spacetimedb_lib::sats::{self, bsatn};
use spacetimedb_lib::{bsatn::ToBsatn as _, ProductValue};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_table::page_pool::PagePool;
use spacetimedb_testing::modules::{Csharp, ModuleLanguage, Rust};
use std::sync::Arc;
use std::sync::OnceLock;

#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn criterion_benchmark(c: &mut Criterion) {
    serialize_benchmarks::<u32_u64_str>(c);
    serialize_benchmarks::<u32_u64_u64>(c);
    serialize_benchmarks::<u64_u64_u32>(c);

    custom_benchmarks::<Rust>(c);
    custom_benchmarks::<Csharp>(c);
}

fn custom_benchmarks<L: ModuleLanguage>(c: &mut Criterion) {
    let db = SpacetimeModule::<L>::build(true).unwrap();

    custom_module_benchmarks(&db, c);
    custom_db_benchmarks(&db, c);
}

fn custom_module_benchmarks<L: ModuleLanguage>(m: &SpacetimeModule<L>, c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("special/{}", SpacetimeModule::<L>::name()));

    let args = sats::product!["0".repeat(65536).into_boxed_str()];
    group.bench_function("large_arguments/64KiB", |b| {
        b.to_async(m)
            .iter(|| async { m.module.call_reducer_binary("fn_with_1_args", &args).await.unwrap() })
    });

    for n in [1u32, 100, 1000] {
        let args = sats::product![n];
        group.bench_function(format!("print_bulk/lines={n}"), |b| {
            b.to_async(m)
                .iter(|| async { m.module.call_reducer_binary("print_many_things", &args).await.unwrap() })
        });
    }
}

fn custom_db_benchmarks<L: ModuleLanguage>(m: &SpacetimeModule<L>, c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("special/db_game/{}", L::NAME));
    // This bench take long, so adjust for it
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);

    let init_db: OnceLock<()> = OnceLock::new();
    for n in [10, 100] {
        let args = sats::product![n];
        group.bench_function(format!("circles/load={n}"), |b| {
            // Initialize outside the benchmark so the db is seed once, to avoid to enlarge the db
            init_db.get_or_init(|| {
                m.block_on(async {
                    m.module
                        .call_reducer_binary("init_game_circles", &sats::product![100])
                        .await
                        .unwrap()
                });
            });

            b.to_async(m)
                .iter(|| async { m.module.call_reducer_binary("run_game_circles", &args).await.unwrap() })
        });
    }

    let init_db: OnceLock<()> = OnceLock::new();
    for n in [10, 100] {
        let args = sats::product![n];
        group.bench_function(format!("ia_loop/load={n}"), |b| {
            // Initialize outside the benchmark so the db is seed once, to avoid `unique` constraints violations
            init_db.get_or_init(|| {
                m.block_on(async {
                    m.module
                        .call_reducer_binary("init_game_ia_loop", &sats::product![500])
                        .await
                        .unwrap();
                })
            });

            b.to_async(m)
                .iter(|| async { m.module.call_reducer_binary("run_game_ia_loop", &args).await.unwrap() })
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

    group.bench_function(format!("product_value/count={count}"), |b| {
        b.iter_batched(
            || data.clone(),
            |data| data.into_iter().map(|row| row.into_product_value()).collect::<Vec<_>>(),
            criterion::BatchSize::PerIteration,
        );
    });
    // this measures serialization from a ProductValue, not directly (as in, from generated code in the Rust SDK.)
    let data_pv = &data
        .into_iter()
        .map(|row| spacetimedb_lib::AlgebraicValue::Product(row.into_product_value()))
        .collect::<ProductValue>();

    group.bench_function(format!("bsatn/count={count}"), |b| {
        b.iter(|| sats::bsatn::to_vec(data_pv).unwrap());
    });
    group.bench_function(format!("json/count={count}"), |b| {
        b.iter(|| serde_json::to_string(data_pv).unwrap());
    });

    let mut table_schema = TableSchema::from_product_type(T::product_type());
    table_schema.table_name = name.into();
    let mut table = spacetimedb_table::table::Table::new(
        Arc::new(table_schema),
        spacetimedb_table::indexes::SquashedOffset::COMMITTED_STATE,
    );
    let pool = PagePool::new_for_test();
    let mut blob_store = spacetimedb_table::blob_store::HashMapBlobStore::default();

    let ptrs = data_pv
        .elements
        .iter()
        .map(|row| {
            table
                .insert(&pool, &mut blob_store, row.as_product().unwrap())
                .unwrap()
                .1
                .pointer()
        })
        .collect::<Vec<_>>();
    let refs = ptrs
        .into_iter()
        .map(|ptr| table.get_row_ref(&blob_store, ptr).unwrap())
        .collect::<Vec<_>>();
    group.bench_function(format!("bflatn_to_bsatn_slow_path/count={count}"), |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            for row_ref in &refs {
                bsatn::to_writer(&mut buf, row_ref).unwrap();
            }
            buf
        })
    });
    group.bench_function(format!("bflatn_to_bsatn_fast_path/count={count}"), |b| {
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

    group.bench_function(format!("bsatn/count={count}"), |b| {
        b.iter(|| bsatn::from_slice::<Vec<T>>(&data_bin).unwrap());
    });
    group.bench_function(format!("json/count={count}"), |b| {
        b.iter(|| serde_json::from_str::<Vec<T>>(&data_json).unwrap());
    });
    // TODO: deserialize benches (needs a typespace)
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
