use criterion::{black_box, criterion_group, criterion_main, Criterion};
use spacetimedb::error::DBError;
use spacetimedb::execution_context::Workload;
use spacetimedb::host::module_host::DatabaseTableUpdate;
use spacetimedb::identity::AuthCtx;
use spacetimedb::messages::websocket::BsatnFormat;
use spacetimedb::sql::ast::SchemaViewer;
use spacetimedb::subscription::query::compile_read_only_queryset;
use spacetimedb::subscription::subscription::ExecutionSet;
use spacetimedb::subscription::tx::DeltaTx;
use spacetimedb::subscription::{collect_table_update, TableUpdateType};
use spacetimedb::{db::relational_db::RelationalDB, messages::websocket::Compression};
use spacetimedb_bench::database::BenchDatabase as _;
use spacetimedb_bench::spacetime_raw::SpacetimeRaw;
use spacetimedb_execution::pipelined::PipelinedProject;
use spacetimedb_primitives::{col_list, TableId};
use spacetimedb_query::compile_subscription;
use spacetimedb_sats::{bsatn, product, AlgebraicType, AlgebraicValue, ProductValue};

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

fn insert_op(table_id: TableId, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
    DatabaseTableUpdate {
        table_id,
        table_name: table_name.into(),
        inserts: [row].into(),
        deletes: [].into(),
    }
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

    let new_lhs_row = product!(entity_id, owner, footprint);
    let new_rhs_row = product!(entity_id, chunk_index, x, z, dimension);

    let ins_lhs = insert_op(lhs, "footprint", new_lhs_row);
    let ins_rhs = insert_op(rhs, "location", new_rhs_row);
    let update = [&ins_lhs, &ins_rhs];

    // A benchmark runner for the new query engine
    let bench_query = |c: &mut Criterion, name, sql| {
        c.bench_function(name, |b| {
            let tx = raw.db.begin_tx(Workload::Subscribe);
            let auth = AuthCtx::for_testing();
            let schema_viewer = &SchemaViewer::new(&tx, &auth);
            let (plan, table_id, table_name, _) = compile_subscription(sql, schema_viewer, &auth).unwrap();
            let plan = plan.optimize().map(PipelinedProject::from).unwrap();
            let tx = DeltaTx::from(&tx);

            b.iter(|| {
                drop(black_box(collect_table_update::<_, BsatnFormat>(
                    &plan,
                    table_id,
                    table_name.clone(),
                    Compression::None,
                    &tx,
                    TableUpdateType::Subscribe,
                )))
            })
        });
    };

    let bench_eval = |c: &mut Criterion, name, sql| {
        c.bench_function(name, |b| {
            let tx = raw.db.begin_tx(Workload::Update);
            let query = compile_read_only_queryset(&raw.db, &AuthCtx::for_testing(), &tx, sql).unwrap();
            let query: ExecutionSet = query.into();

            b.iter(|| {
                drop(black_box(query.eval::<BsatnFormat>(
                    &raw.db,
                    &tx,
                    None,
                    Compression::None,
                )))
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

    // To profile this benchmark for 30s
    // samply record -r 10000000 cargo bench --bench=subscription --profile=profiling -- full-scan --exact --profile-time=30
    // Iterate 1M rows.
    bench_eval(c, "full-scan", "select * from footprint");

    // To profile this benchmark for 30s
    // samply record -r 10000000 cargo bench --bench=subscription --profile=profiling -- full-join --exact --profile-time=30
    // Join 1M rows on the left with 12K rows on the right.
    // Note, this should use an index join so as not to read the entire footprint table.
    let name = format!(
        r#"
        select footprint.*
        from footprint join location on footprint.entity_id = location.entity_id
        where location.chunk_index = {chunk_index}
        "#
    );
    bench_eval(c, "full-join", &name);

    // To profile this benchmark for 30s
    // samply record -r 10000000 cargo bench --bench=subscription --profile=profiling -- incr-select --exact --profile-time=30
    c.bench_function("incr-select", |b| {
        // A passthru executed independently of the database.
        let select_lhs = "select * from footprint";
        let select_rhs = "select * from location";
        let tx = &raw.db.begin_tx(Workload::Update);
        let query_lhs = compile_read_only_queryset(&raw.db, &AuthCtx::for_testing(), tx, select_lhs).unwrap();
        let query_rhs = compile_read_only_queryset(&raw.db, &AuthCtx::for_testing(), tx, select_rhs).unwrap();
        let query = ExecutionSet::from_iter(query_lhs.into_iter().chain(query_rhs));
        let tx = &tx.into();

        b.iter(|| drop(black_box(query.eval_incr_for_test(&raw.db, tx, &update, None))))
    });

    // To profile this benchmark for 30s
    // samply record -r 10000000 cargo bench --bench=subscription --profile=profiling -- incr-join --exact --profile-time=30
    c.bench_function("incr-join", |b| {
        // Not a passthru - requires reading of database state.
        let join = format!(
            "\
            select footprint.* \
            from footprint join location on footprint.entity_id = location.entity_id \
            where location.chunk_index = {chunk_index}"
        );
        let tx = &raw.db.begin_tx(Workload::Update);
        let query = compile_read_only_queryset(&raw.db, &AuthCtx::for_testing(), tx, &join).unwrap();
        let query: ExecutionSet = query.into();
        let tx = &tx.into();

        b.iter(|| drop(black_box(query.eval_incr_for_test(&raw.db, tx, &update, None))));
    });

    // To profile this benchmark for 30s
    // samply record -r 10000000 cargo bench --bench=subscription --profile=profiling -- query-indexes-multi --exact --profile-time=30
    // Iterate 1M rows.
    bench_eval(
        c,
        "query-indexes-multi",
        "select * from location WHERE x = 0 AND z = 10000 AND dimension = 0",
    );
}

criterion_group!(benches, eval);
criterion_main!(benches);
