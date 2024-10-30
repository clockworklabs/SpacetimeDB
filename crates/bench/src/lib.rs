pub mod database;
pub mod schemas;
pub mod spacetime_module;
pub mod spacetime_raw;
pub mod sqlite;

pub type ResultBench<T> = Result<T, anyhow::Error>;

#[cfg(test)]
mod test_basic_invariants {
    use crate::{
        database::BenchDatabase,
        schemas::{create_sequential, u32_u64_str, u32_u64_u64, BenchTable, IndexStrategy, RandomTable},
        spacetime_module::{BenchModule, CSharp, Rust, SpacetimeModule},
        spacetime_raw::SpacetimeRaw,
        sqlite::SQLite,
        ResultBench,
    };
    use serial_test::serial;
    use std::{io, path::Path, sync::Once};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    static INIT: Once = Once::new();

    fn prepare_tests() {
        INIT.call_once(|| {
            // logs. see SpacetimeDB\crates\core\src\startup.rs
            let timer = tracing_subscriber::fmt::time();
            let format = tracing_subscriber::fmt::format::Format::default()
                .with_timer(timer)
                .with_line_number(true)
                .with_file(true)
                .with_target(false)
                .compact();
            let fmt_layer = tracing_subscriber::fmt::Layer::default()
                .event_format(format)
                .with_writer(io::stdout);
            let env_filter_layer = tracing_subscriber::EnvFilter::from_default_env();
            tracing_subscriber::Registry::default()
                .with(fmt_layer)
                .with(env_filter_layer)
                .init();

            // Remove cached data from previous runs.
            // This directory is only reused to speed up runs with Callgrind. In tests, it's fine to wipe it.
            let mut bench_dot_spacetime = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
            bench_dot_spacetime.push(".spacetime");
            if std::fs::metadata(&bench_dot_spacetime).is_ok() {
                std::fs::remove_dir_all(bench_dot_spacetime)
                    .expect("failed to wipe Spacetimedb/crates/bench/.spacetime");
            }
        });
    }

    fn basic_invariants<DB: BenchDatabase, T: BenchTable + RandomTable>(
        index_strategy: IndexStrategy,
        in_memory: bool,
    ) -> ResultBench<()> {
        prepare_tests();

        let mut db = DB::build(in_memory, false)?;
        let table_id = db.create_table::<T>(index_strategy)?;
        assert_eq!(db.count_table(&table_id)?, 0, "tables should begin empty");

        // Chosen arbitrarily.
        let count = 37;

        let sample_data = create_sequential::<T>(0xdeadbeef, count, 100);

        db.insert_bulk(&table_id, sample_data.clone())?;
        assert_eq!(db.count_table(&table_id)?, count, "inserted rows should be inserted");

        db.clear_table(&table_id)?;
        assert_eq!(
            db.count_table(&table_id)?,
            0,
            "clearing the table should clear the table"
        );

        db.insert_bulk(&table_id, sample_data.clone())?;
        assert_eq!(
            db.count_table(&table_id)?,
            count,
            "bulk inserted rows should be bulk inserted"
        );

        if index_strategy == IndexStrategy::Unique0 {
            db.update_bulk::<T>(&table_id, count)?;
            assert_eq!(
                db.count_table(&table_id)?,
                count,
                "bulk updated rows should be bulk updated"
            );
        }

        db.clear_table(&table_id)?;
        assert_eq!(
            db.count_table(&table_id)?,
            0,
            "clearing the table should clear the table"
        );
        Ok(())
    }

    #[test]
    fn test_basic_invariants_sqlite() {
        basic_invariants::<SQLite, u32_u64_str>(IndexStrategy::Unique0, true).unwrap();
        basic_invariants::<SQLite, u32_u64_u64>(IndexStrategy::Unique0, true).unwrap();
        basic_invariants::<SQLite, u32_u64_str>(IndexStrategy::BTreeEachColumn, true).unwrap();
        basic_invariants::<SQLite, u32_u64_u64>(IndexStrategy::BTreeEachColumn, true).unwrap();
    }

    #[test]
    fn test_basic_invariants_spacetime_raw() {
        basic_invariants::<SpacetimeRaw, u32_u64_str>(IndexStrategy::Unique0, true).unwrap();
        basic_invariants::<SpacetimeRaw, u32_u64_u64>(IndexStrategy::Unique0, true).unwrap();
        basic_invariants::<SpacetimeRaw, u32_u64_str>(IndexStrategy::BTreeEachColumn, true).unwrap();
        basic_invariants::<SpacetimeRaw, u32_u64_u64>(IndexStrategy::BTreeEachColumn, true).unwrap();
    }

    fn test_basic_invariants_spacetime_module<M: BenchModule>() {
        // note: there can only be one #[test] invoking spacetime module stuff.
        // #[test]s run concurrently and they fight over lockfiles.
        // so, run the sub-tests here in sequence.
        basic_invariants::<SpacetimeModule<M>, u32_u64_str>(IndexStrategy::Unique0, true).unwrap();
        basic_invariants::<SpacetimeModule<M>, u32_u64_u64>(IndexStrategy::Unique0, true).unwrap();
        basic_invariants::<SpacetimeModule<M>, u32_u64_str>(IndexStrategy::BTreeEachColumn, true).unwrap();
        basic_invariants::<SpacetimeModule<M>, u32_u64_u64>(IndexStrategy::BTreeEachColumn, true).unwrap();
    }

    #[test]
    #[serial]
    fn test_basic_invariants_spacetime_module_rust() {
        test_basic_invariants_spacetime_module::<Rust>();
    }

    #[test]
    #[serial]
    fn test_basic_invariants_spacetime_module_csharp() {
        test_basic_invariants_spacetime_module::<CSharp>();
    }
}
