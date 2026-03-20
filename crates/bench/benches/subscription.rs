use criterion::{black_box, criterion_group, criterion_main, Criterion};
use spacetimedb::client::consume_each_list::ConsumeEachBuffer;
use spacetimedb::db::relational_db::RelationalDB;
use spacetimedb::error::DBError;
use spacetimedb::identity::AuthCtx;
use spacetimedb::sql::ast::SchemaViewer;
use spacetimedb::subscription::row_list_builder_pool::BsatnRowListBuilderPool;
use spacetimedb::subscription::tx::DeltaTx;
use spacetimedb::subscription::{collect_table_update, TableUpdateType};
use spacetimedb_bench::database::BenchDatabase as _;
use spacetimedb_bench::spacetime_raw::SpacetimeRaw;
use spacetimedb_client_api_messages::websocket::v1::BsatnFormat;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_execution::pipelined::PipelinedProject;
use spacetimedb_primitives::{col_list, TableId};
use spacetimedb_query::compile_subscription;
use spacetimedb_sats::{bsatn, product, AlgebraicType, AlgebraicValue};
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn create_table_location(db: &RelationalDB) -> Result<TableId, DBError> {
    let schema = &[
        ("entity_id", AlgebraicType::U64),
        ("chunk_index", AlgebraicType::U64),
        ("x", AlgebraicType::I32),
        ("z", AlgebraicType::I32),
        ("dimension", AlgebraicType::U32),
    ];
    let indexes = &[0.into(), 1.into()];

    // Is necessary to test for both single & multi-column indexes...
    db.create_table_for_test_mix_indexes("location", schema, indexes, col_list![2, 3, 4])
}

fn create_table_footprint(db: &RelationalDB) -> Result<TableId, DBError> {
    let footprint = AlgebraicType::sum(["A", "B", "C", "D"].map(|n| (n, AlgebraicType::unit())));
    let schema = &[
        ("entity_id", AlgebraicType::U64),
        ("owner_entity_id", AlgebraicType::U64),
        ("type", footprint),
    ];
    let indexes = &[0.into(), 1.into()];
    db.create_table_for_test("footprint", schema, indexes)
}

fn eval(c: &mut Criterion) {
    let raw = SpacetimeRaw::build(false).unwrap();

    let lhs = create_table_footprint(&raw.db).unwrap();
    let rhs = create_table_location(&raw.db).unwrap();

    //TODO: Change this to `Workload::ForTest` once `#[cfg(bench)]` is stabilized.
    let _ = raw
        .db
        .with_auto_commit(Workload::Internal, |tx| -> Result<(), DBError> {
            // 1M rows
            let mut scratch = Vec::new();
            for entity_id in 0u64..1_000_000 {
                let owner = entity_id % 1_000;
                let footprint = AlgebraicValue::sum(entity_id as u8 % 4, AlgebraicValue::unit());
                let row = product!(entity_id, owner, footprint);

                scratch.clear();
                bsatn::to_writer(&mut scratch, &row).unwrap();
                let _ = raw.db.insert(tx, lhs, &scratch)?;
            }
            Ok(())
        });

    let _ = raw
        .db
        .with_auto_commit(Workload::Internal, |tx| -> Result<(), DBError> {
            // 1000 chunks, 1200 rows per chunk = 1.2M rows
            let mut scratch = Vec::new();
            for chunk_index in 0u64..1_000 {
                for i in 0u64..1200 {
                    let entity_id = chunk_index * 1200 + i;
                    let x = 0i32;
                    let z = entity_id as i32;
                    let dimension = 0u32;
                    let row = product!(entity_id, chunk_index, x, z, dimension);

                    scratch.clear();
                    bsatn::to_writer(&mut scratch, &row).unwrap();
                    let _ = raw.db.insert(tx, rhs, &scratch)?;
                }
            }
            Ok(())
        });

    let entity_id = 1_200_000u64;
    let chunk_index = 5u64;
    let x = 0i32;
    let z = 0i32;
    let dimension = 0u32;

    let footprint = AlgebraicValue::sum(1, AlgebraicValue::unit());
    let owner = 6u64;

    let _new_lhs_row = product!(entity_id, owner, footprint);
    let _new_rhs_row = product!(entity_id, chunk_index, x, z, dimension);

    let bsatn_rlb_pool = black_box(BsatnRowListBuilderPool::new());

    // A benchmark runner for the subscription engine.
    let bench_query = |c: &mut Criterion, name, sql| {
        c.bench_function(name, |b| {
            let tx = raw.db.begin_tx(Workload::Subscribe);
            let auth = AuthCtx::for_testing();
            let schema_viewer = &SchemaViewer::new(&tx, &auth);
            let (plans, table_id, table_name, _) = compile_subscription(sql, schema_viewer, &auth).unwrap();
            let plans = plans
                .into_iter()
                .map(|plan| plan.optimize(&auth).unwrap())
                .map(PipelinedProject::from)
                .collect::<Vec<_>>();
            let tx = DeltaTx::from(&tx);

            b.iter(|| {
                let updates = black_box(collect_table_update::<BsatnFormat>(
                    &plans,
                    table_id,
                    table_name.clone(),
                    &tx,
                    TableUpdateType::Subscribe,
                    &bsatn_rlb_pool,
                ));
                if let Ok((updates, _)) = updates {
                    updates.consume_each_list(&mut |buffer| bsatn_rlb_pool.try_put(buffer));
                }
            })
        });
    };

    // Join 1M rows on the left with 12K rows on the right.
    // Note, this should use an index join so as not to read the entire footprint table.
    let semijoin = format!(
        r#"
        select f.*
        from footprint f join location l on f.entity_id = l.entity_id
        where l.chunk_index = {chunk_index}
        "#
    );

    let index_scan_multi = "select * from location WHERE x = 0 AND z = 10000 AND dimension = 0";

    bench_query(c, "footprint-scan", "select * from footprint");
    bench_query(c, "footprint-semijoin", &semijoin);
    bench_query(c, "index-scan-multi", index_scan_multi);
}

criterion_group!(benches, eval);
criterion_main!(benches);
