#![allow(clippy::disallowed_names)]
use std::time::Duration;

use spacetimedb::spacetimedb_lib::db::raw_def::v9::TableAccess;
use spacetimedb::spacetimedb_lib::{self, bsatn};
use spacetimedb::{
    duration, table, ConnectionId, Deserialize, Identity, ReducerContext, SpacetimeType, Table, Timestamp,
};
use spacetimedb::{log, ProcedureContext};

pub type TestAlias = TestA;

// ─────────────────────────────────────────────────────────────────────────────
// TABLE DEFINITIONS
// ─────────────────────────────────────────────────────────────────────────────

#[spacetimedb::table(name = person, public, index(name = age, btree(columns = [age])))]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u32,
    name: String,
    age: u8,
}

#[spacetimedb::table(name = test_a, index(name = foo, btree(columns = [x])))]
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

const DEFAULT_TEST_C: TestC = TestC::Foo;
#[table(name = test_d, public)]
pub struct TestD {
    #[default(Some(DEFAULT_TEST_C))]
    test_c: Option<TestC>,
}

// uses internal apis that should not be used by user code
#[allow(dead_code)] // false positive
const fn get_table_access<Tbl: spacetimedb::Table>(_: impl Fn(&spacetimedb::Local) -> &Tbl + Copy) -> TableAccess {
    <Tbl as spacetimedb::table::TableInternal>::TABLE_ACCESS
}

// This table was specified as public.
const _: () = assert!(matches!(get_table_access(test_d::test_d), TableAccess::Public));

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
pub struct Baz {
    pub field: String,
}

#[derive(SpacetimeType)]
pub enum Foobar {
    Baz(Baz),
    Bar,
    Har(u32),
}

#[table(name = test_f, public)]
pub struct TestFoobar {
    pub field: Foobar,
}

#[derive(SpacetimeType)]
#[sats(name = "Namespace.TestF")]
pub enum TestF {
    Foo,
    Bar,
    Baz(String),
}

// // All tables are private by default.
const _: () = assert!(matches!(get_table_access(test_e::test_e), TableAccess::Private));

// FIXME: Table named "private" doesn't compile in C#
// Must be commented here because the schemas are compared between Rust and C#
// in the testing.
// #[spacetimedb::table(name = private)]
// pub struct Private {
//     name: String,
// }

#[spacetimedb::table(name = private_table, private)]
pub struct PrivateTable {
    name: String,
}

#[spacetimedb::table(name = points, private, index(name = multi_column_index, btree(columns = [x, y])))]
pub struct Point {
    x: i64,
    y: i64,
}

// It is redundant, but we can explicitly specify a table as private.
const _: () = assert!(matches!(get_table_access(points::points), TableAccess::Private));

// Test we can compile multiple constraints
#[spacetimedb::table(name = pk_multi_identity)]
struct PkMultiIdentity {
    #[primary_key]
    id: u32,
    #[unique]
    #[auto_inc]
    other: u32,
}

// #[spacetimedb::migrate]
// pub fn migrate() {}

#[spacetimedb::table(name = repeating_test_arg, scheduled(repeating_test))]
pub struct RepeatingTestArg {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    prev_time: Timestamp,
}

#[spacetimedb::table(name = has_special_stuff)]
pub struct HasSpecialStuff {
    identity: Identity,
    connection_id: ConnectionId,
}

/// These two tables defined with the same row type
/// verify that we can define multiple tables with the same type.
///
/// In the past, we've had issues where each `#[table]` attribute
/// would try to emit its own `impl` block for `SpacetimeType` (and some other traits),
/// resulting in duplicate/conflicting trait definitions.
/// See e.g. [SpacetimeDB issue #2097](https://github.com/clockworklabs/SpacetimeDB/issues/2097).
#[spacetimedb::table(public, name = player)]
#[spacetimedb::table(public, name = logged_out_player)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    #[auto_inc]
    #[unique]
    player_id: u64,
    #[unique] // fields called "name" previously caused name collisions in generated table handles
    name: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// SUPPORT TYPES
// ─────────────────────────────────────────────────────────────────────────────

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
// ─────────────────────────────────────────────────────────────────────────────
// REDUCERS
// ─────────────────────────────────────────────────────────────────────────────

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.repeating_test_arg().insert(RepeatingTestArg {
        prev_time: ctx.timestamp,
        scheduled_id: 0,
        scheduled_at: duration!("1000ms").into(),
    });
}

#[spacetimedb::reducer]
pub fn repeating_test(ctx: &ReducerContext, arg: RepeatingTestArg) {
    let delta_time = ctx
        .timestamp
        .duration_since(arg.prev_time)
        .expect("arg.prev_time is later than ctx.timestamp... huh?");
    log::trace!("Timestamp: {:?}, Delta time: {:?}", ctx.timestamp, delta_time);
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String, age: u8) {
    ctx.db.person().insert(Person { id: 0, name, age });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}

#[spacetimedb::reducer]
pub fn list_over_age(ctx: &ReducerContext, age: u8) {
    for person in ctx.db.person().age().filter(age..) {
        log::info!("{} has age {} >= {}", person.name, person.age, age);
    }
}

#[spacetimedb::reducer]
fn log_module_identity(ctx: &ReducerContext) {
    log::info!("Module identity: {}", ctx.identity());
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
        TestF::Baz(string) => log::info!("{string}"),
    }
    for i in 0..1000 {
        ctx.db.test_a().insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: "Yo".to_owned(),
        });
    }

    let row_count_before_delete = ctx.db.test_a().count();

    log::info!("Row count before delete: {row_count_before_delete:?}");

    let mut num_deleted = 0;
    for row in 5..10u32 {
        num_deleted += ctx.db.test_a().foo().delete(row);
    }

    let row_count_after_delete = ctx.db.test_a().count();

    if row_count_before_delete != row_count_after_delete + num_deleted {
        log::error!(
            "Started with {row_count_before_delete} rows, deleted {num_deleted}, and wound up with {row_count_after_delete} rows... huh?",
        );
    }

    match ctx.db.test_e().try_insert(TestE {
        id: 0,
        name: "Tyler".to_owned(),
    }) {
        Ok(x) => log::info!("Inserted: {x:?}"),
        Err(err) => log::info!("Error: {err:?}"),
    }

    log::info!("Row count after delete: {row_count_after_delete:?}");

    let other_row_count = ctx
        .db
        .test_a()
        // .iter()
        // .filter(|row| row.x >= 0 && row.x <= u32::MAX)
        .count();

    log::info!("Row count filtered by condition: {other_row_count:?}");

    log::info!("MultiColumn");

    for i in 0i64..1000 {
        ctx.db.points().insert(Point {
            x: i + arg.x as i64,
            y: i + arg.y as i64,
        });
    }

    let multi_row_count = ctx.db.points().iter().filter(|row| row.x >= 0 && row.y <= 200).count();

    log::info!("Row count filtered by multi-column condition: {multi_row_count:?}");

    log::info!("END");
    Ok(())
}

#[spacetimedb::reducer]
pub fn add_player(ctx: &ReducerContext, name: String) -> Result<(), String> {
    // This always creates a new one because id is auto-incremented.
    let inserted = ctx.db.test_e().id().try_insert_or_update(TestE { id: 0, name })?;

    // Since the previous one is always inserted, at this point it's always updated by this function.
    // This is a no-op, but we can still call it.
    ctx.db.test_e().id().try_insert_or_update(inserted)?;

    Ok(())
}

#[spacetimedb::reducer]
pub fn delete_player(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    if ctx.db.test_e().id().delete(id) {
        Ok(())
    } else {
        Err(format!("No TestE row with id {id}"))
    }
}

#[spacetimedb::reducer]
pub fn delete_players_by_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    match ctx.db.test_e().name().delete(&name) {
        0 => Err(format!("No TestE row with name {name:?}")),
        num_deleted => {
            log::info!("Deleted {num_deleted} player(s) with name {name:?}");
            Ok(())
        }
    }
}

#[spacetimedb::reducer(client_connected)]
fn client_connected(_ctx: &ReducerContext) {}

#[spacetimedb::reducer]
pub fn add_private(ctx: &ReducerContext, name: String) {
    ctx.db.private_table().insert(PrivateTable { name });
}

#[spacetimedb::reducer]
pub fn query_private(ctx: &ReducerContext) {
    for person in ctx.db.private_table().iter() {
        log::info!("Private, {}!", person.name);
    }
    log::info!("Private, World!");
}

#[spacetimedb::reducer]
/// This reducer tests many of the different ways we want to provide arguments to btree index accessors.
///
/// The runtime behavior is not tested, but we have a CI job which asserts that this module compiles,
/// and therefore that all of the different accesses listed here are well-typed.
// TODO(testing): Add tests (in smoketests?) for index arg combos which are expected not to compile.
fn test_btree_index_args(ctx: &ReducerContext) {
    // Single-column string index on `test_e.name`:
    // Tests that we can pass `&String` or `&str`, but not `str`.
    let string = "String".to_string();
    let _ = ctx.db.test_e().name().filter(&string);
    let _ = ctx.db.test_e().name().filter("str");

    // let _filter_by_owned_string = ctx.db.test_e().name().filter(string); // SHOULD FAIL

    ctx.db.test_e().name().delete(&string);
    ctx.db.test_e().name().delete("str");

    // ctx.db.test_e().name().delete(string); // SHOULD FAIL

    // Multi-column i64 index on `points.x, points.y`:
    // Tests that we can pass various ranges
    // and various combinations of borrowed/owned `Copy` values.
    // TODO: Why is the `i64` suffix required here?

    // A single non-range value, owned or by reference.
    let _ = ctx.db.points().multi_column_index().filter(0i64);
    let _ = ctx.db.points().multi_column_index().filter(&0i64);

    // A single tuple of a non-range value, owned or by reference..
    let _ = ctx.db.points().multi_column_index().filter((0i64,));
    let _ = ctx.db.points().multi_column_index().filter((&0i64,));

    // Ranges of owned values.
    let _ = ctx.db.points().multi_column_index().filter(0i64..3i64);
    let _ = ctx.db.points().multi_column_index().filter(0i64..=3i64);
    let _ = ctx.db.points().multi_column_index().filter(0i64..);
    let _ = ctx.db.points().multi_column_index().filter(..3i64);
    let _ = ctx.db.points().multi_column_index().filter(..=3i64);

    // Ranges of references.
    let _ = ctx.db.points().multi_column_index().filter(&0i64..&3i64);
    let _ = ctx.db.points().multi_column_index().filter(&0i64..=&3i64);
    let _ = ctx.db.points().multi_column_index().filter(&0i64..);
    let _ = ctx.db.points().multi_column_index().filter(..&3i64);
    let _ = ctx.db.points().multi_column_index().filter(..=&3i64);

    // A single tuple of a range of owned values.
    let _ = ctx.db.points().multi_column_index().filter((0i64..3i64,));
    let _ = ctx.db.points().multi_column_index().filter((0i64..=3i64,));
    let _ = ctx.db.points().multi_column_index().filter((0i64..,));
    let _ = ctx.db.points().multi_column_index().filter((..3i64,));
    let _ = ctx.db.points().multi_column_index().filter((..=3i64,));

    // A single tuple of a range of references.
    let _ = ctx.db.points().multi_column_index().filter((&0i64..&3i64,));
    let _ = ctx.db.points().multi_column_index().filter((&0i64..=&3i64,));
    let _ = ctx.db.points().multi_column_index().filter((&0i64..,));
    let _ = ctx.db.points().multi_column_index().filter((..&3i64,));
    let _ = ctx.db.points().multi_column_index().filter((..=&3i64,));

    // Non-range values for both columns.
    let _ = ctx.db.points().multi_column_index().filter((0i64, 1i64));
    let _ = ctx.db.points().multi_column_index().filter((&0i64, 1i64));
    let _ = ctx.db.points().multi_column_index().filter((0i64, &1i64));
    let _ = ctx.db.points().multi_column_index().filter((&0i64, &1i64));

    // A non-range value in the first column and a range in the second.
    // We're trusting that all of the different range types listed above still work here.
    let _ = ctx.db.points().multi_column_index().filter((0i64, 1i64..3i64));
    let _ = ctx.db.points().multi_column_index().filter((&0i64, 1i64..3i64));
    let _ = ctx.db.points().multi_column_index().filter((0i64, &1i64..&3i64));
    let _ = ctx.db.points().multi_column_index().filter((&0i64, &1i64..&3i64));

    // ctx.db.points().multi_column_index().filter((0i64..3i64, 1i64)); // SHOULD FAIL
}

#[spacetimedb::reducer]
fn assert_caller_identity_is_module_identity(ctx: &ReducerContext) {
    let caller = ctx.sender;
    let owner = ctx.identity();
    if caller != owner {
        panic!("Caller {caller} is not the owner {owner}");
    } else {
        log::info!("Called by the owner {owner}");
    }
}

#[spacetimedb::procedure]
fn sleep_one_second(ctx: &mut ProcedureContext) {
    let prev_time = ctx.timestamp;
    let target = prev_time + Duration::from_secs(1);
    ctx.sleep_until(target);
    let new_time = ctx.timestamp;
    let actual_delta = new_time.duration_since(prev_time).unwrap();
    log::info!("Slept from {prev_time} to {new_time}, a total of {actual_delta:?}");
}

#[spacetimedb::procedure]
fn return_value(_ctx: &mut ProcedureContext, foo: u64) -> Baz {
    Baz {
        field: format!("{foo}"),
    }
}
