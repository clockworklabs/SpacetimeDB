#![allow(clippy::disallowed_names)]
use spacetimedb::spacetimedb_lib::db::auth::StAccess;
use spacetimedb::spacetimedb_lib::{self, bsatn};
use spacetimedb::{duration, table, Address, Deserialize, Identity, ReducerContext, SpacetimeType, Table, Timestamp};

#[spacetimedb::table(name = test_a, index(name = foo, btree(columns = [x])))]
pub struct TestA {
    #[index(btree)]
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

#[table(name = test_d, public)]
pub struct TestD {
    test_c: Option<TestC>,
}

// uses internal apis that should not be used by user code
#[allow(dead_code)] // false positive
const fn get_table_access<T: spacetimedb::table::__MapRowTypeToTable>() -> StAccess {
    <T::Table<'static> as spacetimedb::table::TableInternal>::TABLE_ACCESS
}

// This table was specified as public.
const _: () = assert!(matches!(get_table_access::<TestD>(), StAccess::Public));

#[spacetimedb::table(name = test_e)]
#[derive(Debug)]
pub struct TestE {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    name: String,
}

#[derive(SpacetimeType)]
#[sats(name = "Namespace.TestF")]
pub enum TestF {
    Foo,
    Bar,
    Baz(String),
}

// // All tables are private by default.
const _: () = assert!(matches!(get_table_access::<TestE>(), StAccess::Private));

#[spacetimedb::table(name = private)]
pub struct Private {
    name: String,
}

#[spacetimedb::table(name = points, private, index(name = multi_column_index, btree(columns = [x, y])))]
pub struct Point {
    x: i64,
    y: i64,
}

// It is redundant, but we can explicitly specify a table as private.
const _: () = assert!(matches!(get_table_access::<Point>(), StAccess::Private));

// Test we can compile multiple constraints
#[spacetimedb::table(name = pk_multi_identity)]
struct PkMultiIdentity {
    #[primary_key]
    id: u32,
    #[unique]
    #[auto_inc]
    other: u32,
}
pub type TestAlias = TestA;

// #[spacetimedb::migrate]
// pub fn migrate() {}

#[spacetimedb::table(name = repeating_test_arg, scheduled(repeating_test))]
pub struct RepeatingTestArg {
    prev_time: Timestamp,
}

#[spacetimedb::table(name = has_special_stuff)]
pub struct HasSpecialStuff {
    identity: Identity,
    address: Address,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.repeating_test_arg().insert(RepeatingTestArg {
        prev_time: Timestamp::now(),
        scheduled_id: 0,
        scheduled_at: duration!("1000ms").into(),
    });
}

#[spacetimedb::reducer]
pub fn repeating_test(ctx: &ReducerContext, arg: RepeatingTestArg) {
    let delta_time = arg.prev_time.elapsed();
    log::trace!("Timestamp: {:?}, Delta time: {:?}", ctx.timestamp, delta_time);
}

#[spacetimedb::reducer]
pub fn test(ctx: &ReducerContext, arg: TestAlias, arg2: TestB, arg3: TestC, arg4: TestF) -> anyhow::Result<()> {
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
        ctx.db.test_a().insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: "Yo".to_owned(),
        });
    }

    let row_count_before_delete = ctx.db.test_a().count();

    log::info!("Row count before delete: {:?}", row_count_before_delete);

    let mut num_deleted = 0;
    for row in 5..10 {
        num_deleted += ctx.db.test_a().foo().delete(row);
    }

    let row_count_after_delete = ctx.db.test_a().count();

    if row_count_before_delete != row_count_after_delete + num_deleted {
        log::error!(
            "Started with {} rows, deleted {}, and wound up with {} rows... huh?",
            row_count_before_delete,
            num_deleted,
            row_count_after_delete,
        );
    }

    match ctx.db.test_e().try_insert(TestE {
        id: 0,
        name: "Tyler".to_owned(),
    }) {
        Ok(x) => log::info!("Inserted: {:?}", x),
        Err(err) => log::info!("Error: {:?}", err),
    }

    log::info!("Row count after delete: {:?}", row_count_after_delete);

    let other_row_count = ctx
        .db
        .test_a()
        // .iter()
        // .filter(|row| row.x >= 0 && row.x <= u32::MAX)
        .count();

    log::info!("Row count filtered by condition: {:?}", other_row_count);

    log::info!("MultiColumn");

    for i in 0i64..1000 {
        ctx.db.points().insert(Point {
            x: i + arg.x as i64,
            y: i + arg.y as i64,
        });
    }

    let multi_row_count = ctx.db.points().iter().filter(|row| row.x >= 0 && row.y <= 200).count();

    log::info!("Row count filtered by multi-column condition: {:?}", multi_row_count);

    log::info!("END");
    Ok(())
}

#[spacetimedb::reducer]
pub fn add_player(ctx: &ReducerContext, name: String) -> Result<(), String> {
    ctx.db.test_e().try_insert(TestE { id: 0, name })?;
    Ok(())
}

#[spacetimedb::reducer]
pub fn delete_player(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    if ctx.db.test_e().id().delete(id) {
        Ok(())
    } else {
        Err(format!("No TestE row with id {}", id))
    }
}

#[spacetimedb::reducer]
pub fn delete_players_by_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    match ctx.db.test_e().name().delete(&name) {
        0 => Err(format!("No TestE row with name {:?}", name)),
        num_deleted => {
            log::info!("Deleted {} player(s) with name {:?}", num_deleted, name);
            Ok(())
        }
    }
}

#[spacetimedb::reducer(client_connected)]
fn on_connect(_ctx: &ReducerContext) {}

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

#[spacetimedb::reducer]
pub fn add_private(ctx: &ReducerContext, name: String) {
    ctx.db.private().insert(Private { name });
}

#[spacetimedb::reducer]
pub fn query_private(ctx: &ReducerContext) {
    for person in ctx.db.private().iter() {
        log::info!("Private, {}!", person.name);
    }
    log::info!("Private, World!");
}
