#![allow(clippy::disallowed_names)]

use spacetimedb::{
    delete_by_col_eq, query, spacetimedb, AlgebraicValue, Deserialize, ReducerContext, SpacetimeType, Timestamp,
};
use spacetimedb_lib::bsatn;

// Define a SpacetimeDB table with an index and fields.
#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "foo", x))]
pub struct TestA {
    pub x: u32,
    pub y: u32,
    pub z: String,
}

// Define a struct for TestB.
#[derive(SpacetimeType)]
pub struct TestB {
    foo: String,
}

// Define an enum for TestC with specific serialization settings.
#[derive(SpacetimeType)]
#[sats(name = "Namespace.TestC")]
pub enum TestC {
    // Foo(String),
    Foo,
    Bar,
}

// Define a table for TestD with an optional TestC field.
#[spacetimedb(table)]
pub struct TestD {
    test_c: Option<TestC>,
}

// Define a table for TestE with a primary key and auto-incrementing ID.
#[spacetimedb(table)]
#[derive(Debug)]
pub struct TestE {
    #[primarykey]
    #[autoinc]
    id: u64,
    name: String,
}

// Define a private table (not accessible outside this module).
pub struct _Private {
    name: String,
}

// Create a type alias for TestA.
pub type TestAlias = TestA;

// SpacetimeDB initialization function.
#[spacetimedb(init)]
pub fn init() {
    // Schedule a repeating task every 1000ms.
    spacetimedb::schedule!("1000ms", repeating_test(_, Timestamp::now()));
}

// SpacetimeDB update function.
#[spacetimedb(update)]
pub fn update() {
    log::info!("Update called!");
}

// SpacetimeDB repeating reducer function.
#[spacetimedb(reducer, repeat = 1000ms)]
pub fn repeating_test(ctx: ReducerContext, prev_time: Timestamp) {
    let delta_time = prev_time.elapsed();
    log::trace!("Timestamp: {:?}, Delta time: {:?}", ctx.timestamp, delta_time);
}

// SpacetimeDB reducer function with multiple arguments.
#[spacetimedb(reducer)]
pub fn test(ctx: ReducerContext, arg: TestAlias, arg2: TestB, arg3: TestC) -> anyhow::Result<()> {
    log::info!("BEGIN");
    log::info!("sender: {:?}", ctx.sender);
    log::info!("timestamp: {:?}", ctx.timestamp);
    log::info!("bar: {:?}", arg2.foo);

    match arg3 {
        // TestC::Foo(string) => log::info!("{}", string),
        TestC::Foo => log::info!("Foo"),
        TestC::Bar => log::info!("Bar"),
    }

    // Insert test data into TestA.
    for i in 0..10 {
        TestA::insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: "Yo".to_owned(),
        });
    }

    // Get the row count before delete.
    let row_count = TestA::iter().count();
    log::info!("Row count before delete: {:?}", row_count);

    // Delete rows in TestA.
    for row in 5..10 {
        delete_by_col_eq(1, 0, &AlgebraicValue::U32(row))?;
    }

    // Get the row count after delete.
    let row_count = TestA::iter().count();

    // Insert data into TestE and handle errors.
    match TestE::insert(TestE {
        id: 0,
        name: "Tyler".to_owned(),
    }) {
        Ok(x) => log::info!("Inserted: {:?}", x),
        Err(err) => log::info!("Error: {:?}", err),
    }

    log::info!("Row count after delete: {:?}", row_count);

    // Count rows in TestA based on a condition.
    let other_row_count = query!(|row: TestA| row.x >= 0 && row.x <= u32::MAX).count();
    log::info!("Row count filtered by condition: {:?}", other_row_count);

    log::info!("END");
    Ok(())
}

// SpacetimeDB reducer function to add a player to TestE.
#[spacetimedb(reducer)]
pub fn add_player(name: String) -> Result<(), String> {
    TestE::insert(TestE { id: 0, name })?;
    Ok(())
}

// SpacetimeDB on-connect function.
#[spacetimedb(connect)]
fn on_connect(_ctx: ReducerContext) {}

// Define a struct Foo with generic lifetimes and derive Deserialize for it.
#[derive(Deserialize)]
pub struct Foo<'a> {
    pub field: &'a str,
}

impl Foo<'_> {
    // A function to deserialize data into Foo.
    pub fn baz(data: &[u8]) -> Foo<'_> {
        bsatn::from_slice(data).unwrap()
    }
}

// SpacetimeDB reducer function to add a private entry.
#[spacetimedb(reducer)]
pub fn add_private(name: String) {
    _Private::insert(_Private { name });
}

// SpacetimeDB reducer function to query private entries.
#[spacetimedb(reducer)]
pub fn query_private() {
    for person in _Private::iter() {
        log::info!("Private, {}!", person.name);
    }
    log::info!("Private, World!");
}
