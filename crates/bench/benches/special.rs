use criterion::{criterion_group, criterion_main, Criterion};
use mimalloc::MiMalloc;
use spacetimedb::db::{Config, Storage};
use spacetimedb_bench::{
    schemas::{create_sequential, BenchTable, Location, Person, RandomTable},
    spacetime_module::BENCHMARKS_MODULE,
};
use spacetimedb_lib::{sats, ProductValue};
use spacetimedb_testing::modules::start_runtime;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn criterion_benchmark(c: &mut Criterion) {
    serialize_benchmarks::<Person>(c);
    serialize_benchmarks::<Location>(c);

    custom_module_benchmarks(c);
}

fn custom_module_benchmarks(c: &mut Criterion) {
    let runtime = start_runtime();

    let config = Config {
        storage: Storage::Memory,
        fsync: spacetimedb::db::FsyncPolicy::Never,
    };
    let module = runtime.block_on(async { BENCHMARKS_MODULE.load_module(config, None).await });

    let args = sats::product!["0".repeat(65536)];
    c.bench_function("stdb_module/large_arguments/64KiB", |b| {
        b.iter_batched(
            || args.clone(),
            |args| runtime.block_on(async { module.call_reducer_binary("fn_with_1_args", args).await.unwrap() }),
            criterion::BatchSize::PerIteration,
        )
    });

    for n in [1u32, 100, 1000] {
        let args = sats::product![n];
        c.bench_function(&format!("stdb_module/print_bulk/lines={n}"), |b| {
            b.iter_batched(
                || args.clone(),
                |args| runtime.block_on(async { module.call_reducer_binary("print_many_things", args).await.unwrap() }),
                criterion::BatchSize::PerIteration,
            )
        });
    }
}

fn serialize_benchmarks<T: BenchTable + RandomTable>(c: &mut Criterion) {
    let name = T::name_snake_case();
    let count = 100;
    let mut group = c.benchmark_group("special/serialize");
    group.throughput(criterion::Throughput::Elements(count));

    let data = create_sequential::<T>(0xdeadbeef, count as u32, 100);

    group.bench_function(&format!("{name}/product_value/count={count}"), |b| {
        b.iter_batched(
            || data.clone(),
            |data| data.into_iter().map(|row| row.into_product_value()).collect::<Vec<_>>(),
            criterion::BatchSize::PerIteration,
        );
    });
    // this measures serialization from a ProductValue, not directly (as in, from generated code in the Rust SDK.)
    let data_pv = ProductValue {
        elements: data
            .into_iter()
            .map(|row| spacetimedb_lib::AlgebraicValue::Product(row.into_product_value()))
            .collect::<Vec<_>>(),
    };

    group.bench_function(&format!("{name}/bsatn/count={count}"), |b| {
        b.iter_batched_ref(
            || data_pv.clone(),
            |data_pv| sats::bsatn::to_vec(data_pv).unwrap(),
            criterion::BatchSize::PerIteration,
        );
    });
    group.bench_function(&format!("{name}/json/count={count}"), |b| {
        b.iter_batched_ref(
            || data_pv.clone(),
            |data_pv| serde_json::to_string(data_pv).unwrap(),
            criterion::BatchSize::PerIteration,
        );
    });

    // TODO: deserialize benches (needs a typespace)
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
