#[cfg(target_os = "linux")]
mod callgrind_benches {

    /// Benchmarks that run under our iai-callgrind fork (https://github.com/clockworklabs/iai-callgrind)
    /// Callgrind (https://valgrind.org/docs/manual/cl-manual.html) disassembles linux binaries
    /// as they run to insert instrumentation code. This allows for benchmarking with minimal variance,
    /// compared to timing benchmarks.
    ///
    /// Note: you CAN'T save state between these benchmarks, since each is run
    /// in a fresh process.
    ///
    /// FIXME(jgilles): many of the spacetime_module benchmarks are currently disabled due to a bad interaction with sled (issue #564).
    ///
    /// There is some odd boilerplate in this file that is used to make downstream processing of these benchmarks easier.
    /// Every benchmark is a struct that implements serde::Deserialize; these structs contain some redundant information.
    /// Benchmarks are actually dispatched using iai-callgrind's `#[library_benchmark]` macro.
    /// We pass benchmark configurations as JSON strings to these benchmarks.
    /// This JSON ends up in iai-callgrind's output file; we parse all relevant information out of it.
    /// (In the `crate/src/bin/summarize.rs` binary.)
    use iai_callgrind::{library_benchmark, library_benchmark_group, LibraryBenchmarkConfig};
    use serde::Deserialize;

    use spacetimedb_bench::{
        database::BenchDatabase,
        schemas::{create_partly_identical, create_sequential, u32_u64_str, BenchTable, IndexStrategy, RandomTable},
        spacetime_raw::SpacetimeRaw,
        sqlite::SQLite,
    };
    use spacetimedb_lib::{AlgebraicType, ProductValue};

    /// A benchmark.
    ///
    /// The benchmark information consists of metadata that can be deserialized from JSON.
    /// The JSON is embedded in the output generated by iai-callgrind, which
    /// makes writing downstream tools a lot easier.
    ///
    // TODO(jgilles): use this infra in the non-callgrind benches as well.
    trait Benchmark: Deserialize<'static> {
        fn run_benchmark(self);
    }

    // ========================= INSERT BULK =========================

    #[derive(Deserialize)]
    struct InsertBulkBenchmark<DB: BenchDatabase, T: BenchTable + RandomTable> {
        bench: String,
        db: String,
        in_memory: bool,
        schema: String,
        indices: IndexStrategy,
        preload: u32,
        count: u32,
        #[serde(skip)]
        _marker: std::marker::PhantomData<(DB, T)>,
    }

    impl<DB: BenchDatabase, T: BenchTable + RandomTable> Benchmark for InsertBulkBenchmark<DB, T> {
        fn run_benchmark(self) {
            assert_eq!(self.bench, "insert bulk", "provided metadata has incorrect bench name");
            assert_eq!(self.db, DB::name(), "provided metadata has incorrect db name");
            assert_eq!(self.schema, T::name(), "provided metadata has incorrect db name");

            let mut db = DB::build(self.in_memory, false).unwrap();

            let table_id = db.create_table::<T>(self.indices).unwrap();
            let mut data = create_sequential::<T>(0xdeadbeef, self.count + self.preload, 64);
            let to_preload = data.split_off(self.count as usize);

            // warm up
            db.insert_bulk(&table_id, data.clone()).unwrap();
            db.clear_table(&table_id).unwrap();

            // add preload data
            db.insert_bulk(&table_id, to_preload).unwrap();

            // measure
            spacetimedb::callgrind_flag::enable_callgrind_globally(|| {
                db.insert_bulk(&table_id, data).unwrap();
            });

            // clean up
            db.clear_table(&table_id).unwrap();
        }
    }

    #[library_benchmark]
    #[bench::mem_unique_64(r#"{"bench":"insert bulk", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "preload": 128, "count": 64}"#)]
    #[bench::mem_btree_each_column_64(r#"{"bench":"insert bulk", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "btree_each_column", "preload": 128, "count": 64}"#)]
    #[bench::disk_unique_64(r#"{"bench":"insert bulk", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "preload": 128, "count": 64}"#)]
    #[bench::disk_btree_each_column_64(r#"{"bench":"insert bulk", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "btree_each_column", "preload": 128, "count": 64}"#)]
    fn insert_bulk_raw_u32_u64_str(metadata: &str) {
        let bench: InsertBulkBenchmark<SpacetimeRaw, u32_u64_str> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    #[library_benchmark]
    #[bench::mem_unique_64(r#"{"bench":"insert bulk", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "preload": 128, "count": 64}"#)]
    #[bench::mem_btree_each_column_64(r#"{"bench":"insert bulk", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "btree_each_column", "preload": 128, "count": 64}"#)]
    #[bench::disk_unique_64(r#"{"bench":"insert bulk", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "preload": 128, "count": 64}"#)]
    #[bench::disk_btree_each_column_64(r#"{"bench":"insert bulk", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "btree_each_column", "preload": 128, "count": 64}"#)]
    fn insert_bulk_sqlite_u32_u64_str(metadata: &str) {
        let bench: InsertBulkBenchmark<SQLite, u32_u64_str> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    library_benchmark_group!(
        name = insert_bulk_group;
        benchmarks = insert_bulk_raw_u32_u64_str, /*insert_bulk_module_u32_u64_str,*/ insert_bulk_sqlite_u32_u64_str
    );

    // ========================= UPDATE BULK =========================

    #[derive(Deserialize)]
    struct UpdateBulkBenchmark<DB: BenchDatabase, T: BenchTable + RandomTable> {
        bench: String,
        db: String,
        in_memory: bool,
        schema: String,
        indices: IndexStrategy,
        preload: u32,
        count: u32,
        #[serde(skip)]
        _marker: std::marker::PhantomData<(DB, T)>,
    }

    impl<DB: BenchDatabase, T: BenchTable + RandomTable> Benchmark for UpdateBulkBenchmark<DB, T> {
        fn run_benchmark(self) {
            assert_eq!(self.bench, "update bulk", "provided metadata has incorrect bench name");
            assert_eq!(self.db, DB::name(), "provided metadata has incorrect db name");
            assert_eq!(self.schema, T::name(), "provided metadata has incorrect db name");
            assert_eq!(
                self.indices,
                IndexStrategy::Unique0,
                "provided metadata has incorrect index strategy"
            );

            let mut db = DB::build(self.in_memory, false).unwrap();

            let table_id = db.create_table::<T>(self.indices).unwrap();
            let data = create_sequential::<T>(0xdeadbeef, self.preload, 64);

            // warm up
            db.insert_bulk(&table_id, data).unwrap();
            db.update_bulk::<T>(&table_id, self.count).unwrap();

            // measure
            spacetimedb::callgrind_flag::enable_callgrind_globally(|| {
                db.update_bulk::<T>(&table_id, self.count).unwrap();
            });

            // clean up
            db.clear_table(&table_id).unwrap();
        }
    }

    #[library_benchmark]
    #[bench::mem_64(r#"{"bench":"update bulk", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "preload": 128, "count": 64}"#)]
    #[bench::mem_1024(r#"{"bench":"update bulk", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "preload": 1024, "count": 1024}"#)]
    #[bench::disk_64(r#"{"bench":"update bulk", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "preload": 128, "count": 64}"#)]
    #[bench::disk_1024(r#"{"bench":"update bulk", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "preload": 1024, "count": 1024}"#)]
    fn update_bulk_raw_u32_u64_str(metadata: &str) {
        let bench: UpdateBulkBenchmark<SpacetimeRaw, u32_u64_str> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    #[library_benchmark]
    #[bench::mem_64(r#"{"bench":"update bulk", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "preload": 128, "count": 64}"#)]
    #[bench::mem_1024(r#"{"bench":"update bulk", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "preload": 1024, "count": 1024}"#)]
    #[bench::disk_64(r#"{"bench":"update bulk", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "preload": 128, "count": 64}"#)]
    #[bench::disk_1024(r#"{"bench":"update bulk", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "preload": 1024, "count": 1024}"#)]
    fn update_bulk_sqlite_u32_u64_str(metadata: &str) {
        let bench: UpdateBulkBenchmark<SQLite, u32_u64_str> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    library_benchmark_group!(
        name = update_bulk_group;
        benchmarks = update_bulk_raw_u32_u64_str, /*update_bulk_module_u32_u64_str,*/ update_bulk_sqlite_u32_u64_str
    );

    // ========================= ITERATE =========================

    #[derive(Deserialize)]
    struct IterateBenchmark<DB: BenchDatabase, T: BenchTable + RandomTable> {
        bench: String,
        db: String,
        in_memory: bool,
        schema: String,
        indices: IndexStrategy,
        count: u32,
        #[serde(skip)]
        _marker: std::marker::PhantomData<(DB, T)>,
    }

    impl<DB: BenchDatabase, T: BenchTable + RandomTable> Benchmark for IterateBenchmark<DB, T> {
        fn run_benchmark(self) {
            assert_eq!(self.bench, "iterate", "provided metadata has incorrect bench name");
            assert_eq!(self.db, DB::name(), "provided metadata has incorrect db name");
            assert_eq!(self.schema, T::name(), "provided metadata has incorrect db name");

            let mut db = DB::build(self.in_memory, false).unwrap();

            let table_id = db.create_table::<T>(self.indices).unwrap();
            let data = create_sequential::<T>(0xdeadbeef, self.count, 64);

            // warm up
            db.insert_bulk(&table_id, data).unwrap();
            db.iterate(&table_id).unwrap();

            // measure
            spacetimedb::callgrind_flag::enable_callgrind_globally(|| {
                db.iterate(&table_id).unwrap();
            });

            // clean up
            db.clear_table(&table_id).unwrap();
        }
    }

    #[library_benchmark]
    #[bench::mem_64(
        r#"{"bench": "iterate", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "count": 64}"#
    )]
    #[bench::mem_1024(
        r#"{"bench": "iterate", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "count": 1024}"#
    )]
    #[bench::disk_64(
        r#"{"bench": "iterate", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "count": 64}"#
    )]
    #[bench::disk_1024(
        r#"{"bench": "iterate", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "count": 1024}"#
    )]
    fn iterate_raw_u32_u64_str(metadata: &str) {
        let bench: IterateBenchmark<SpacetimeRaw, u32_u64_str> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    #[library_benchmark]
    #[bench::mem_64(
        r#"{"bench": "iterate", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "count": 64}"#
    )]
    #[bench::mem_1024(
        r#"{"bench": "iterate", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "unique_0", "count": 1024}"#
    )]
    #[bench::disk_64(
        r#"{"bench": "iterate", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "count": 64}"#
    )]
    #[bench::disk_1024(
        r#"{"bench": "iterate", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "unique_0", "count": 1024}"#
    )]
    fn iterate_sqlite_u32_u64_str(metadata: &str) {
        let bench: IterateBenchmark<SQLite, u32_u64_str> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    library_benchmark_group!(
        name = iterate_group;
        benchmarks = iterate_raw_u32_u64_str, /*iterate_module_u32_u64_str,*/ iterate_sqlite_u32_u64_str
    );

    // ========================= FILTER =========================

    #[derive(Deserialize)]
    struct FilterBenchmark<DB: BenchDatabase, T: BenchTable + RandomTable> {
        bench: String,
        db: String,
        in_memory: bool,
        schema: String,
        indices: IndexStrategy,
        count: u32,
        preload: u32,
        // Underscore here cause it's an implementation detail.
        // The only thing downstream cares about is data_type
        _column: u32,
        data_type: String,
        #[serde(skip)]
        _marker: std::marker::PhantomData<(DB, T)>,
    }

    impl<DB: BenchDatabase, T: BenchTable + RandomTable> Benchmark for FilterBenchmark<DB, T> {
        fn run_benchmark(self) {
            assert_eq!(self.bench, "filter", "provided metadata has incorrect bench name");
            assert_eq!(self.db, DB::name(), "provided metadata has incorrect db name");
            assert_eq!(self.schema, T::name(), "provided metadata has incorrect db name");

            let filter_column_type = match T::product_type().elements[self._column as usize].algebraic_type {
                AlgebraicType::String => "string",
                AlgebraicType::U32 => "u32",
                AlgebraicType::U64 => "u64",
                _ => unimplemented!(),
            };
            assert_eq!(
                filter_column_type, self.data_type,
                "provided metadata has incorrect data type"
            );

            let mut db = DB::build(self.in_memory, false).unwrap();

            let table_id = db.create_table::<T>(self.indices).unwrap();
            let data = create_partly_identical::<T>(0xdeadbeef, self.count as u64, self.preload as u64);

            // create_partly_identical guarantees that the first `count` elements will be identical
            let filter_value = data[0].clone().into_product_value().elements[self._column as usize].clone();

            // warm up
            db.insert_bulk(&table_id, data).unwrap();
            db.filter::<T>(&table_id, self._column, filter_value.clone()).unwrap();

            // measure
            spacetimedb::callgrind_flag::enable_callgrind_globally(|| {
                db.filter::<T>(&table_id, self._column, filter_value.clone()).unwrap();
            });

            // clean up
            db.clear_table(&table_id).unwrap();
        }
    }

    #[library_benchmark]
    #[bench::string_mem_no_index_64_from_128(
        r#"{"bench": "filter", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "no_index",
        "count": 64, "preload": 128, "_column": 2, "data_type": "string"}"#
    )]
    #[bench::string_mem_btree_64_from_128(
        r#"{"bench": "filter", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "btree_each_column",
        "count": 64, "preload": 128, "_column": 2, "data_type": "string"}"#
    )]
    #[bench::u64_mem_no_index_64_from_128(
        r#"{"bench": "filter", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "no_index",
        "count": 64, "preload": 128, "_column": 1, "data_type": "u64"}"#
    )]
    #[bench::u64_mem_btree_64_from_128(
        r#"{"bench": "filter", "db": "stdb_raw", "in_memory": true, "schema": "u32_u64_str", "indices": "btree_each_column",
        "count": 64, "preload": 128, "_column": 1, "data_type": "u64"}"#
    )]
    #[bench::string_disk_no_index_64_from_128(
        r#"{"bench": "filter", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "no_index",
        "count": 64, "preload": 128, "_column": 2, "data_type": "string"}"#
    )]
    #[bench::string_disk_btree_64_from_128(
    r#"{"bench": "filter", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "btree_each_column",
        "count": 64, "preload": 128, "_column": 2, "data_type": "string"}"#
    )]
    #[bench::u64_disk_no_index_64_from_128(
        r#"{"bench": "filter", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "no_index",
        "count": 64, "preload": 128, "_column": 1, "data_type": "u64"}"#
    )]
    #[bench::u64_disk_btree_64_from_128(
    r#"{"bench": "filter", "db": "stdb_raw", "in_memory": false, "schema": "u32_u64_str", "indices": "btree_each_column",
        "count": 64, "preload": 128, "_column": 1, "data_type": "u64"}"#
    )]
    fn filter_raw_u32_u64_str(metadata: &str) {
        let bench: FilterBenchmark<SpacetimeRaw, u32_u64_str> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    #[library_benchmark]
    // string, btree index
    #[bench::string_mem_no_index_64_from_128(
        r#"{"bench": "filter", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "no_index",
        "count": 64, "preload": 128, "_column": 2, "data_type": "string"}"#
    )]
    // string, btree index
    #[bench::string_mem_btree_64_from_128(
        r#"{"bench": "filter", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "btree_each_column",
        "count": 64, "preload": 128, "_column": 2, "data_type": "string"}"#
    )]
    // u64, no index
    #[bench::u64_mem_no_index_64_from_128(
        r#"{"bench": "filter", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "no_index",
        "count": 64, "preload": 128, "_column": 1, "data_type": "u64"}"#
    )]
    // u64, btree index
    #[bench::u64_mem_btree_64_from_128(
        r#"{"bench": "filter", "db": "sqlite", "in_memory": true, "schema": "u32_u64_str", "indices": "btree_each_column",
        "count": 64, "preload": 128, "_column": 1, "data_type": "u64"}"#
    )]
    // string, no index
    #[bench::string_disk_no_index_64_from_128(
        r#"{"bench": "filter", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "no_index",
        "count": 64, "preload": 128, "_column": 2, "data_type": "string"}"#
    )]
    // string, btree index
    #[bench::string_disk_btree_64_from_128(
        r#"{"bench": "filter", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "btree_each_column",
        "count": 64, "preload": 128, "_column": 2, "data_type": "string"}"#
    )]
    // u64, no index
    #[bench::u64_disk_no_index_64_from_128(
        r#"{"bench": "filter", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "no_index",
        "count": 64, "preload": 128, "_column": 1, "data_type": "u64"}"#
    )]
    // u64, btree index
    #[bench::u64_disk_btree_64_from_128(
        r#"{"bench": "filter", "db": "sqlite", "in_memory": false, "schema": "u32_u64_str", "indices": "btree_each_column",
        "count": 64, "preload": 128, "_column": 1, "data_type": "u64"}"#
    )]
    fn filter_sqlite_u32_u64_str(metadata: &str) {
        let bench: FilterBenchmark<SQLite, u32_u64_str> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    library_benchmark_group!(
        name = filter_group;
        benchmarks = filter_raw_u32_u64_str, /*filter_module_u32_u64_str,*/ filter_sqlite_u32_u64_str
    );

    // ========================= FIND =========================

    #[derive(Deserialize)]
    struct FindBenchmark<DB: BenchDatabase, T: BenchTable + RandomTable> {
        bench: String,
        db: String,
        schema: String,
        indices: IndexStrategy,
        preload: u32,
        #[serde(skip)]
        _marker: std::marker::PhantomData<(DB, T)>,
    }

    impl<DB: BenchDatabase, T: BenchTable + RandomTable> Benchmark for FindBenchmark<DB, T> {
        fn run_benchmark(self) {
            assert_eq!(self.bench, "find", "provided metadata has incorrect bench name");
            assert_eq!(self.db, DB::name(), "provided metadata has incorrect db name");
            assert_eq!(self.schema, T::name(), "provided metadata has incorrect db name");
            assert_eq!(
                T::product_type().elements[0].algebraic_type,
                AlgebraicType::U32,
                "primary key in tuple slot 0 must be u32"
            );

            let mut db = DB::build(false, false).unwrap();

            let table_id = db.create_table::<T>(self.indices).unwrap();

            let data = create_sequential::<T>(0xdeadbeef, self.preload, 64);

            let filter_value = data[(self.preload - 1) as usize].clone().into_product_value().elements[0].clone();

            // warm up
            db.insert_bulk(&table_id, data).unwrap();
            db.filter::<T>(&table_id, 0, filter_value.clone()).unwrap();

            // measure
            spacetimedb::callgrind_flag::enable_callgrind_globally(|| {
                db.filter::<T>(&table_id, 0, filter_value.clone()).unwrap();
            });

            // clean up
            db.clear_table(&table_id).unwrap();
        }
    }

    // ========================= EMPTY TRANSACTION =========================

    #[derive(Deserialize)]
    struct EmptyTransactionBenchmark<DB: BenchDatabase> {
        bench: String,
        db: String,
        in_memory: bool,
        #[serde(skip)]
        _marker: std::marker::PhantomData<DB>,
    }

    impl<DB: BenchDatabase> Benchmark for EmptyTransactionBenchmark<DB> {
        fn run_benchmark(self) {
            assert_eq!(
                self.bench, "empty transaction",
                "provided metadata has incorrect bench name"
            );
            assert_eq!(self.db, DB::name(), "provided metadata has incorrect db name");

            let mut db = DB::build(self.in_memory, false).unwrap();

            // warm up
            db.empty_transaction().unwrap();

            // measure
            spacetimedb::callgrind_flag::enable_callgrind_globally(|| db.empty_transaction().unwrap());
        }
    }

    #[library_benchmark]
    #[bench::empty_in_mem(r#"{"bench": "empty transaction", "db": "stdb_raw", "in_memory": true}"#)]
    #[bench::empty_on_disk(r#"{"bench": "empty transaction", "db": "stdb_raw", "in_memory": false}"#)]
    fn empty_transaction_raw(metadata: &str) {
        let bench: EmptyTransactionBenchmark<SpacetimeRaw> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    /*
    #[library_benchmark]
    #[bench::b1(r#"{"bench": "empty transaction", "db": "stdb_module"}"#)]
    fn empty_transaction_module(metadata: &str) {
        let bench: EmptyTransactionBenchmark<SpacetimeModule> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }
    */

    #[library_benchmark]
    #[bench::empty_in_mem(r#"{"bench": "empty transaction", "db": "sqlite", "in_memory": true}"#)]
    #[bench::empty_on_disk(r#"{"bench": "empty transaction", "db": "sqlite", "in_memory": false}"#)]
    fn empty_transaction_sqlite(metadata: &str) {
        let bench: EmptyTransactionBenchmark<SQLite> = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    library_benchmark_group!(
        name = empty_transaction_group;
        benchmarks = empty_transaction_raw, /*empty_transaction_module,*/ empty_transaction_sqlite
    );

    // ========================= SERIALIZATION =========================

    #[derive(Deserialize)]
    struct BSatnSerializationBenchmark {
        bench: String,
        format: String,
        count: u32,
    }

    impl Benchmark for BSatnSerializationBenchmark {
        fn run_benchmark(self) {
            assert_eq!(
                self.bench, "serialize_product_value",
                "provided metadata has incorrect bench name"
            );
            assert_eq!(self.format, "bsatn", "provided metadata has incorrect format");

            let buckets = 64;
            let data = create_sequential::<u32_u64_str>(0xdeadbeef, self.count, buckets)
                .into_iter()
                .map(|row| spacetimedb_lib::AlgebraicValue::Product(row.into_product_value()))
                .collect::<ProductValue>();

            spacetimedb::callgrind_flag::enable_callgrind_globally(|| {
                // don't time deallocation: return this!
                spacetimedb_lib::sats::bsatn::to_vec(&data).unwrap()
            }); // allocation dropped here
        }
    }

    #[library_benchmark]
    #[bench::b1(r#"{"bench": "serialize_product_value", "format": "bsatn", "count": 16}"#)]
    #[bench::b2(r#"{"bench": "serialize_product_value", "format": "bsatn", "count": 64}"#)]
    fn bsatn_serialization(metadata: &str) {
        let bench: BSatnSerializationBenchmark = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    #[derive(Deserialize)]
    struct JSONSerializationBenchmark {
        bench: String,
        format: String,
        count: u32,
    }

    impl Benchmark for JSONSerializationBenchmark {
        fn run_benchmark(self) {
            assert_eq!(
                self.bench, "serialize_product_value",
                "provided metadata has incorrect bench name"
            );
            assert_eq!(self.format, "json", "provided metadata has incorrect format");

            let buckets = 64;
            let data = create_sequential::<u32_u64_str>(0xdeadbeef, self.count, buckets)
                .into_iter()
                .map(|row| spacetimedb_lib::AlgebraicValue::Product(row.into_product_value()))
                .collect::<ProductValue>();

            spacetimedb::callgrind_flag::enable_callgrind_globally(|| {
                // don't time deallocation: return this!
                serde_json::to_string(&data).unwrap()
            }); // allocation dropped here
        }
    }

    #[library_benchmark]
    #[bench::b1(r#"{"bench": "serialize_product_value", "format": "json", "count": 16}"#)]
    #[bench::b2(r#"{"bench": "serialize_product_value", "format": "json", "count": 64}"#)]
    fn json_serialization(metadata: &str) {
        let bench: JSONSerializationBenchmark = serde_json::from_str(metadata).unwrap();
        bench.run_benchmark();
    }

    library_benchmark_group!(
        name = serialize_group;
        benchmarks = bsatn_serialization, json_serialization
    );

    // ========================= HARNESS =========================
    iai_callgrind::main!(
        config = LibraryBenchmarkConfig::default()
                    .pass_through_envs(["HOME", "PATH", "RUST_LOG", "RUST_BACKTRACE"])
                    // THE NEXT LINE IS CRITICAL.
                    // Without this line, this entire file breaks!
                    .with_custom_entry_point("spacetimedb::callgrind_flag::flag");
        library_benchmark_groups = insert_bulk_group, update_bulk_group, filter_group,
                                iterate_group, empty_transaction_group,
                                serialize_group
    );

    // have to re-export `main`, it's not marked as `pub` in the macro
    pub fn run_benches() {
        main();
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    callgrind_benches::run_benches();

    #[cfg(not(target_os = "linux"))]
    println!("Callgrind does not exist for your operating system.");
}
