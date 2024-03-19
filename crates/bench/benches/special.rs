use criterion::{black_box, criterion_group, criterion_main, Criterion};
use itertools::Either;
use mimalloc::MiMalloc;
use spacetimedb::client::messages::{SubscriptionUpdate, SubscriptionUpdateMessage};
use spacetimedb::client::{ClientActorId, ClientConnectionSender, Protocol};
use spacetimedb::db::{Config, Storage};
use spacetimedb::host::module_host::ProtocolDatabaseUpdate;
use spacetimedb::protobuf::client_api::{TableRowOperation, TableUpdate};
use spacetimedb_bench::{
    schemas::{create_sequential, u32_u64_str, u32_u64_u64, BenchTable, RandomTable},
    spacetime_module::BENCHMARKS_MODULE,
};
use spacetimedb_lib::{sats, Identity, ProductValue};
use spacetimedb_sats::bsatn::to_vec;
use spacetimedb_sats::product;
use spacetimedb_testing::modules::start_runtime;
use std::time::Instant;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn criterion_benchmark(c: &mut Criterion) {
    serialize_benchmarks::<u32_u64_str>(c);
    serialize_benchmarks::<u32_u64_u64>(c);

    custom_module_benchmarks(c);
    send_benchmarks(c);
}

fn custom_module_benchmarks(c: &mut Criterion) {
    let runtime = start_runtime();

    let config = Config {
        storage: Storage::Memory,
        fsync: spacetimedb::db::FsyncPolicy::Never,
    };
    let module = runtime.block_on(async { BENCHMARKS_MODULE.load_module(config, None).await });

    let args = sats::product!["0".repeat(65536)];
    c.bench_function("special/stdb_module/large_arguments/64KiB", |b| {
        b.iter_batched(
            || args.clone(),
            |args| runtime.block_on(async { module.call_reducer_binary("fn_with_1_args", args).await.unwrap() }),
            criterion::BatchSize::PerIteration,
        )
    });

    for n in [1u32, 100, 1000] {
        let args = sats::product![n];
        c.bench_function(&format!("special/stdb_module/print_bulk/lines={n}"), |b| {
            b.iter_batched(
                || args.clone(),
                |args| runtime.block_on(async { module.call_reducer_binary("print_many_things", args).await.unwrap() }),
                criterion::BatchSize::PerIteration,
            )
        });
    }
}

fn serialize_benchmarks<T: BenchTable + RandomTable>(c: &mut Criterion) {
    let name = T::name();
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

fn send_benchmarks(c: &mut Criterion) {
    let count = 10_000u64;
    let rows: Vec<_> = (0..count).map(|i| product![i, format!("Row {i}")]).collect();

    let database_update = TableUpdate {
        table_id: 1,
        table_name: "sample".to_string(),
        table_row_operations: rows
            .into_iter()
            .map(|x| TableRowOperation {
                op: 1,
                row: to_vec(&x).unwrap(),
            })
            .collect(),
    };

    let subscription_update = SubscriptionUpdate {
        database_update: ProtocolDatabaseUpdate {
            tables: Either::Left(vec![database_update]),
        },
        request_id: None,
        timer: None,
    };

    let mut group = c.benchmark_group("special/channel");
    group.throughput(criterion::Throughput::Elements(count));

    group.bench_function(&format!("send/count={count}"), |b| {
        b.iter_custom(|iters| {
            let (client, _sendrx) = ClientConnectionSender::dummy_pull(
                ClientActorId::for_test(Identity::ZERO),
                Protocol::Binary,
                iters as usize,
            );

            let start = Instant::now();

            for _i in 0..iters {
                black_box(client.send_message(SubscriptionUpdateMessage {
                    subscription_update: subscription_update.clone(),
                }))
                .unwrap();
            }
            start.elapsed()
        })
    });
    //
    // group.bench_function(&format!("rec/count={count}"), |b| {
    //     b.iter_custom(|iters| {
    //         let (client, mut sendrx) = ClientConnectionSender::dummy_pull(
    //             ClientActorId::for_test(Identity::ZERO),
    //             Protocol::Binary,
    //             iters as usize,
    //         );
    //
    //         for _i in 0..iters {
    //             black_box(
    //                 client
    //                     .send_message(SubscriptionUpdateMessage {
    //                         subscription_update: subscription_update.clone(),
    //                     })
    //                     .unwrap(),
    //             );
    //         }
    //         let handle = tokio::spawn(async move {
    //             let start = Instant::now();
    //             for _i in 0..iters {
    //                 black_box(sendrx.recv()).await;
    //             }
    //
    //             start.elapsed()
    //         });
    //     })
    // });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
