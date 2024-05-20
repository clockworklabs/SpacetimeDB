#![allow(clippy::disallowed_names)]
use spacetimedb::spacetimedb_lib::{self, bsatn};
use spacetimedb::{query, spacetimedb, Deserialize, ReducerContext, SpacetimeType, Timestamp};

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "foo", x))]
pub struct TestA {
    pub x: u32,
    pub y: u32,
    pub z: String,
}

#[derive(SpacetimeType)]
pub struct TestB {
    foo: String,
}

#[derive(SpacetimeType)]
#[sats(name = "Namespace.TestC")]
pub enum TestC {
    // Foo(String),
    Foo,
    Bar,
}

#[spacetimedb(table)]
pub struct TestD {
    test_c: Option<TestC>,
}

#[spacetimedb(table)]
#[derive(Debug)]
pub struct TestE {
    #[primarykey]
    #[autoinc]
    id: u64,
    name: String,
}

#[spacetimedb(table)]
pub struct _Private {
    name: String,
}

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "multi_column_index", x, y))]
pub struct Point {
    x: i64,
    y: i64,
}

// Test we can compile multiple constraints
#[spacetimedb(table)]
struct PkMultiIdentity {
    #[primarykey]
    id: u32,
    #[unique]
    #[autoinc]
    other: u32,
}
pub type TestAlias = TestA;

// #[spacetimedb(migrate)]
// pub fn migrate() {}

#[spacetimedb(init)]
pub fn init() {
    spacetimedb::schedule!("1000ms", repeating_test(_, Timestamp::now()));
}

#[spacetimedb(update)]
pub fn update() {
    log::info!("Update called!");
}

#[spacetimedb(reducer)]
pub fn repeating_test(ctx: ReducerContext, prev_time: Timestamp) {
    let delta_time = prev_time.elapsed();
    log::trace!("Timestamp: {:?}, Delta time: {:?}", ctx.timestamp, delta_time);

    // Reschedule ourselves.
    spacetimedb::schedule!("1000ms", repeating_test(_, Timestamp::now()));
}

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
    for i in 0..1000 {
        TestA::insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: "Yo".to_owned(),
        });
    }

    let row_count_before_delete = TestA::iter().count();

    log::info!("Row count before delete: {:?}", row_count_before_delete);

    let mut num_deleted = 0;
    for row in 5..10 {
        num_deleted += TestA::delete_by_x(&row);
    }

    let row_count_after_delete = TestA::iter().count();

    if row_count_before_delete != row_count_after_delete + num_deleted as usize {
        log::error!(
            "Started with {} rows, deleted {}, and wound up with {} rows... huh?",
            row_count_before_delete,
            num_deleted,
            row_count_after_delete,
        );
    }

    match TestE::insert(TestE {
        id: 0,
        name: "Tyler".to_owned(),
    }) {
        Ok(x) => log::info!("Inserted: {:?}", x),
        Err(err) => log::info!("Error: {:?}", err),
    }

    log::info!("Row count after delete: {:?}", row_count_after_delete);

    let other_row_count = query!(|row: TestA| row.x >= 0 && row.x <= u32::MAX).count();

    log::info!("Row count filtered by condition: {:?}", other_row_count);

    log::info!("MultiColumn");

    for i in 0i64..1000 {
        Point::insert(Point {
            x: i + arg.x as i64,
            y: i + arg.y as i64,
        });
    }

    let multi_row_count = query!(|row: Point| row.x >= 0 && row.y <= 200).count();

    log::info!("Row count filtered by multi-column condition: {:?}", multi_row_count);

    log::info!("END");
    Ok(())
}

#[spacetimedb(reducer)]
pub fn add_player(name: String) -> Result<(), String> {
    TestE::insert(TestE { id: 0, name })?;
    Ok(())
}

#[spacetimedb(reducer)]
pub fn delete_player(id: u64) -> Result<(), String> {
    if TestE::delete_by_id(&id) {
        Ok(())
    } else {
        Err(format!("No TestE row with id {}", id))
    }
}

#[spacetimedb(reducer)]
pub fn delete_players_by_name(name: String) -> Result<(), String> {
    match TestE::delete_by_name(&name) {
        0 => Err(format!("No TestE row with name {:?}", name)),
        num_deleted => {
            log::info!("Deleted {} player(s) with name {:?}", num_deleted, name);
            Ok(())
        }
    }
}

#[spacetimedb(connect)]
fn on_connect(_ctx: ReducerContext) {}

// We can derive `Deserialize` for lifetime generic types:

#[derive(Deserialize)]
pub struct Foo<'a> {
    pub field: &'a str,
}

impl Foo<'_> {
    pub fn baz(data: &[u8]) -> Foo<'_> {
        bsatn::from_slice(data).unwrap()
    }
}

#[spacetimedb(reducer)]
pub fn add_private(name: String) {
    _Private::insert(_Private { name });
}

#[spacetimedb(reducer)]
pub fn query_private() {
    for person in _Private::iter() {
        log::info!("Private, {}!", person.name);
    }
    log::info!("Private, World!");
}
