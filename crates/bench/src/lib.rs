pub mod database;
pub mod schemas;
pub mod spacetime_module;
pub mod spacetime_raw;
pub mod sqlite;

pub type ResultBench<T> = Result<T, anyhow::Error>;

#[cfg(test)]
mod tests {
    use crate::{
        database::BenchDatabase,
        schemas::{create_sequential, BenchTable, Location, Person, RandomTable, TableStyle},
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

    fn sanity<DB: BenchDatabase, T: BenchTable + RandomTable>(
        table_style: TableStyle,
        in_memory: bool,
    ) -> ResultBench<()> {
        prepare_tests();

        let mut db = DB::build(in_memory, false)?;
        let table_id = db.create_table::<T>(table_style)?;
        assert_eq!(db.count_table(&table_id)?, 0, "tables should begin empty");

        let count = 37;

        let sample_data = create_sequential::<T>(0xdeadbeef, count, 100);

        let prepared = db.prepare_insert(&table_id)?;
        for row in sample_data {
            db.insert::<T>(&prepared, row)?;
        }
        assert_eq!(db.count_table(&table_id)?, count, "inserted rows should be inserted");

        db.clear_table(&table_id)?;
        assert_eq!(
            db.count_table(&table_id)?,
            0,
            "clearing the table should clear the table"
        );

        let sample_data = create_sequential::<T>(0xdeadbeef, count, 100);
        let prepared = db.prepare_insert_bulk(&table_id)?;
        db.insert_bulk(&prepared, sample_data.clone())?;
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

        // do it again! had an issue with indexes not getting cleared...
        db.insert_bulk(&prepared, sample_data.clone())?;
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
        drop(db);
        Ok(())
    }

    #[test]
    fn test_sanity_sqlite() {
        sanity::<SQLite, Person>(TableStyle::Unique, true).unwrap();
        sanity::<SQLite, Location>(TableStyle::Unique, true).unwrap();
    }

    #[test]
    fn test_sanity_spacetime_raw() {
        sanity::<SpacetimeRaw, Person>(TableStyle::Unique, true).unwrap();
        sanity::<SpacetimeRaw, Location>(TableStyle::Unique, true).unwrap();
    }

    #[test]
    fn test_sanity_spacetime_module() {
        // note: there can only be one test invoking spacetime module stuff. Otherwise they
        // fight over lockfiles.
        sanity::<SpacetimeModule, Person>(TableStyle::Unique, true).unwrap();
        sanity::<SpacetimeModule, Location>(TableStyle::Unique, true).unwrap();
    }
}
