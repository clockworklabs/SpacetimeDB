#![allow(clippy::disallowed_names)]
use spacetimedb::sats::db::auth::StAccess;
use spacetimedb::spacetimedb_lib::{self, bsatn};
use spacetimedb::{duration, query, spacetimedb, Deserialize, ReducerContext, SpacetimeType, TableType, Timestamp};

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
    Foo,
    Bar,
}

#[spacetimedb(table(public))]
pub struct TestD {
    test_c: Option<TestC>,
}

// This table was specified as public.
const _: () = assert!(matches!(TestD::TABLE_ACCESS, StAccess::Public));

#[spacetimedb(table)]
#[derive(Debug)]
pub struct TestE {
    #[primarykey]
    #[autoinc]
    id: u64,
    name: String,
}

#[derive(SpacetimeType)]
#[sats(name = "Namespace.TestF")]
pub enum TestF {
    Foo,
    Bar,
    Baz(String),
}

// All tables are private by default.
const _: () = assert!(matches!(TestE::TABLE_ACCESS, StAccess::Private));

#[spacetimedb(table)]
pub struct Private {
    name: String,
}

#[spacetimedb(table(private))]
#[spacetimedb(index(btree, name = "multi_column_index", x, y))]
pub struct Point {
    x: i64,
    y: i64,
}

// It is redundant, but we can explicitly specify a table as private.
const _: () = assert!(matches!(Point::TABLE_ACCESS, StAccess::Private));

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

#[spacetimedb(table, scheduled(repeating_test))]
pub struct RepeatingTestArg {
    prev_time: Timestamp,
}

#[spacetimedb(init)]
pub fn init() {
    let _ = RepeatingTestArg::insert(RepeatingTestArg {
        prev_time: Timestamp::now(),
        scheduled_id: 0,
        scheduled_at: duration!("1000ms").into(),
    });
}

#[spacetimedb(update)]
pub fn update() {
    log::info!("Update called!");
}

#[spacetimedb(reducer)]
pub fn repeating_test(ctx: ReducerContext, arg: RepeatingTestArg) {
    let delta_time = arg.prev_time.elapsed();
    log::trace!("Timestamp: {:?}, Delta time: {:?}", ctx.timestamp, delta_time);
}

#[spacetimedb(reducer)]
pub fn test(ctx: ReducerContext, arg: TestAlias, arg2: TestB, arg3: TestC, arg4: TestF) -> anyhow::Result<()> {
    log::info!("BEGIN");
    log::info!("sender: {:?}", ctx.sender);
    log::info!("timestamp: {:?}", ctx.timestamp);
    log::info!("bar: {:?}", arg2.foo);

    match arg3 {
        TestC::Foo => log::info!("Foo"),
        TestC::Bar => log::info!("Bar"),
    }
    match arg4 {
        TestF::Foo => log::info!("Foo"),
        TestF::Bar => log::info!("Bar"),
        TestF::Baz(string) => log::info!("{}", string),
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
    Private::insert(Private { name });
}

#[spacetimedb(reducer)]
pub fn query_private() {
    for person in Private::iter() {
        log::info!("Private, {}!", person.name);
    }
    log::info!("Private, World!");
}
