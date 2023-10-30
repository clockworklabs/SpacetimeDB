use spacetimedb_bench::{
    database::BenchDatabase,
    schemas::{create_sequential, IndexStrategy, Person},
    spacetime_raw,
};

fn main() {
    eprintln!("Preparing!");
    eprintln!("CWD: {}", std::env::current_dir().unwrap().display());
    //let mut db: spacetime_module::SpacetimeModule = BenchDatabase::build(true, false).unwrap();
    let mut db: spacetime_raw::SpacetimeRaw = BenchDatabase::build(false, false).unwrap();
    let data = create_sequential::<Person>(0xdeadbeef, 100, 100);
    let table = db.create_table::<Person>(IndexStrategy::Unique).unwrap();
    db.insert_bulk(&table, data.clone()).unwrap();
    assert_eq!(db.count_table(&table).unwrap(), 100);

    eprintln!("Count successful!");
    db.clear_table(&table).unwrap();

    eprintln!("For real this time, enabling callgrind!");
    spacetimedb::callgrind_flag::enable_callgrind_globally(|| {
        db.insert_bulk(&table, data.clone()).unwrap();
    });
    assert_eq!(db.count_table(&table).unwrap(), 100);
    eprintln!("Count successful!");
    db.clear_table(&table).unwrap();
}
