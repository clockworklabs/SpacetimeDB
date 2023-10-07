use crate::schemas::BenchTable;
use spacetimedb::db::datastore::traits::{ColId, ColumnSchema, TableSchema};
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::sats::string;

pub mod database;
pub mod schemas;
pub mod spacetime_module;
pub mod spacetime_raw;
pub mod sqlite;

pub type ResultBench<T> = Result<T, anyhow::Error>;

pub(crate) fn create_schema<T: BenchTable>(table_name: &str) -> TableSchema {
    let columns = T::product_type()
        .elements
        .into_iter()
        .enumerate()
        .map(|(pos, col)| ColumnSchema {
            table_id: 0,
            col_id: ColId(pos as u32),
            col_name: col.name.unwrap(),
            col_type: col.algebraic_type,
            is_autoinc: false,
        });

    TableSchema {
        table_id: 0,
        table_name: string(table_name),
        indexes: vec![],
        columns: columns.collect(),
        constraints: vec![],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        database::BenchDatabase,
        schemas::{create_sequential, BenchTable, IndexStrategy, Location, Person, RandomTable},
        spacetime_module::SpacetimeModule,
        spacetime_raw::SpacetimeRaw,
        sqlite::SQLite,
        ResultBench,
    };
    use std::{io, sync::Once};
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

        for row in sample_data.clone() {
            db.insert::<T>(&table_id, row)?;
        }
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
        basic_invariants::<SQLite, Person>(IndexStrategy::Unique, true).unwrap();
        basic_invariants::<SQLite, Location>(IndexStrategy::Unique, true).unwrap();
    }

    #[test]
    fn test_basic_invariants_sqlite_multi_index() {
        basic_invariants::<SQLite, Person>(IndexStrategy::MultiIndex, true).unwrap();
        basic_invariants::<SQLite, Location>(IndexStrategy::MultiIndex, true).unwrap();
    }

    #[test]
    fn test_basic_invariants_spacetime_raw() {
        basic_invariants::<SpacetimeRaw, Person>(IndexStrategy::Unique, true).unwrap();
        basic_invariants::<SpacetimeRaw, Location>(IndexStrategy::Unique, true).unwrap();
    }

    #[test]
    fn test_basic_invariants_spacetime_module() {
        // note: there can only be one #[test] invoking spacetime module stuff.
        // #[test]s run concurrently and they fight over lockfiles.
        // so, run the sub-tests here in sequence.
        basic_invariants::<SpacetimeModule, Person>(IndexStrategy::Unique, true).unwrap();
        basic_invariants::<SpacetimeModule, Location>(IndexStrategy::Unique, true).unwrap();
    }
}
