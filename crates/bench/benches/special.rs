use criterion::async_executor::AsyncExecutor;
use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use mimalloc::MiMalloc;
use spacetimedb::db::datastore::traits::IsolationLevel;
use spacetimedb::db::relational_db::tests_utils::{make_snapshot, TestDB};
use spacetimedb::db::relational_db::{open_snapshot_repo, RelationalDB};
use spacetimedb::execution_context::Workload;
use spacetimedb_bench::{
    database::BenchDatabase,
    schemas::{create_sequential, u32_u64_str, u32_u64_u64, u64_u64_u32, BenchTable, RandomTable},
    spacetime_module::SpacetimeModule,
};
use spacetimedb_fs_utils::compression::CompressType;
use spacetimedb_lib::sats::{self, bsatn};
use spacetimedb_lib::{bsatn::ToBsatn as _, Identity, ProductValue};
use spacetimedb_paths::server::{ReplicaDir, SnapshotsPath};
use spacetimedb_paths::FromPathUnchecked;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_snapshot::{SnapshotRepository, SnapshotSize};
use spacetimedb_testing::modules::{Csharp, ModuleLanguage, Rust};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tempdir::TempDir;
use spacetimedb_sats::bsatn::to_vec;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

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

    snapshot(c);
    snapshot_existing(c);
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

fn _snapshot<F>(c: &mut Criterion, name: &str, dir: SnapshotsPath, take: F)
where
    F: Fn(&SnapshotRepository),
{
    let mut disk_size = None;
    let mut size_on_disk = |size: SnapshotSize| {
        if size.compressed_type == CompressType::None {
            // Save the size of the last snapshot to use as throughput
            disk_size = Some(size.clone());
        }
        dbg!(&size);
    };

    let algos = [
        CompressType::None,
        CompressType::Zstd,
        CompressType::Lz4,
        CompressType::Snap,
    ];
    // For show the size of the last snapshot
    for compress in &algos {
        let (_, repo) = make_snapshot(dir.clone(), Identity::ZERO, 0, *compress, true);
        take(&repo);
        size_on_disk(repo.size_on_disk_last_snapshot().unwrap());
    }

    let mut group = c.benchmark_group(&format!("special/snapshot/{name}]"));
    group.throughput(criterion::Throughput::Bytes(disk_size.unwrap().total_size));
    group.sample_size(50);
    group.warm_up_time(Duration::from_secs(10));
    group.sampling_mode(SamplingMode::Flat);

    for compress in &algos {
        group.bench_function(format!("save_compression_{compress:?}"), |b| {
            b.iter_batched(
                || {},
                |_| {
                    let (_, repo) = make_snapshot(dir.clone(), Identity::ZERO, 0, *compress, true);
                    take(&repo);
                },
                criterion::BatchSize::NumIterations(100),
            );
        });

        group.bench_function(format!("open_compression_{compress:?}"), |b| {
            b.iter_batched(
                || {},
                |_| {
                    let (_, repo) = make_snapshot(dir.clone(), Identity::ZERO, 0, *compress, false);
                    let last = repo.latest_snapshot().unwrap().unwrap();
                    repo.read_snapshot(last).unwrap()
                },
                criterion::BatchSize::NumIterations(100),
            );
        });
    }
}

fn snapshot(c: &mut Criterion) {
    let db = TestDB::in_memory().unwrap();

    let dir = db.path().snapshots();
    dir.create().unwrap();
    let mut t1 = TableSchema::from_product_type(u32_u64_str::product_type());
    t1.table_name = "u32_u64_str".into();

    let mut t2 = TableSchema::from_product_type(u32_u64_u64::product_type());
    t2.table_name = "u32_u64_u64".into();

    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    let t1 = db.create_table(&mut tx, t1).unwrap();
    let t2 = db.create_table(&mut tx, t2).unwrap();

    let data = create_sequential::<u32_u64_str>(0xdeadbeef, 1_000, 100);
    for row in data.into_iter() {
        db.insert(&mut tx, t1, &to_vec(&row.into_product_value()).unwrap()).unwrap();
    }

    let data = create_sequential::<u32_u64_u64>(0xdeadbeef, 1_000, 100);
    for row in data.into_iter() {
        db.insert(&mut tx, t2, &to_vec(&row.into_product_value()).unwrap()).unwrap();
    }
    db.commit_tx(tx).unwrap();

    _snapshot(c, "synthetic", dir, |repo| {
        db.take_snapshot(repo).unwrap();
    });
}

// For test compression into an existing database.
// Must supply the path to the database and the identity of the replica using the `ENV`:
// - `SNAPSHOT` the path to the database, like `/tmp/db/replicas/.../8/database`
// - `IDENTITY` the identity in hex format
fn snapshot_existing(c: &mut Criterion) {
    let path_db = if let Ok(path) = std::env::var("SNAPSHOT") {
        PathBuf::from(path)
    } else {
        eprintln!("SNAPSHOT must be set to a valid path to the database");
        return;
    };
    let identity =
        Identity::from_hex(std::env::var("IDENTITY").expect("IDENTITY must be set to a valid hex identity")).unwrap();

    let path = ReplicaDir::from_path_unchecked(path_db);
    let repo = open_snapshot_repo(path.snapshots(), Identity::ZERO, 0).unwrap();

    let last = repo.latest_snapshot().unwrap();
    let db = RelationalDB::restore_from_snapshot_or_bootstrap(identity, Some(&repo), last).unwrap();

    let out = TempDir::new("snapshots").unwrap();

    let dir = SnapshotsPath::from_path_unchecked(out.path());

    _snapshot(c, "existing", dir, |repo| {
        db.take_snapshot(repo).unwrap();
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
