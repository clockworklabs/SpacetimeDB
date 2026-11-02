//! Compare prebuilt C# benchmark modules without rebuilding or changing the host.
use criterion::{async_executor::AsyncExecutor, Criterion};
use spacetimedb_bench::{
    database::BenchDatabase,
    schemas::{create_sequential, u32_u64_str, IndexStrategy},
    spacetime_module::SpacetimeModule,
};
use spacetimedb_lib::sats::product;
use spacetimedb_testing::modules::{CompiledModule, ModuleLanguage};
use std::{path::PathBuf, sync::OnceLock, time::Duration};

struct Artifact;

impl ModuleLanguage for Artifact {
    const NAME: &'static str = "csharp-artifact";

    fn get_module() -> &'static CompiledModule {
        static MODULE: OnceLock<CompiledModule> = OnceLock::new();
        MODULE.get_or_init(|| {
            let path = PathBuf::from(std::env::var_os("CSHARP_BENCH_WASM").expect("set CSHARP_BENCH_WASM"));
            CompiledModule::from_artifact("benchmarks-cs", spacetimedb::messages::control_db::HostType::Wasm, path)
        })
    }
}

fn main() {
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(3))
        .measurement_time(Duration::from_secs(20))
        .configure_from_args();
    let mut db = SpacetimeModule::<Artifact>::build(true).unwrap();
    let table = db.create_table::<u32_u64_str>(IndexStrategy::Unique0).unwrap();
    db.insert_bulk(&table, create_sequential::<u32_u64_str>(0xdeadbeef, 10_000, 100))
        .unwrap();
    criterion.bench_function("csharp-artifact/empty", |b| b.iter(|| db.empty_transaction().unwrap()));
    criterion.bench_function("csharp-artifact/scan-10000", |b| b.iter(|| db.iterate(&table).unwrap()));
    (&db).block_on(async {
        db.module
            .call_reducer_binary("init_game_circles", &product![100u32])
            .await
            .unwrap();
    });
    criterion.bench_function("csharp-artifact/circles-100", |b| {
        b.to_async(&db).iter(|| async {
            db.module
                .call_reducer_binary("run_game_circles", &product![100u32])
                .await
                .unwrap()
        });
    });
    criterion.final_summary();
}
