use spacetimedb::ReducerContext;

struct Test;

#[spacetimedb::reducer]
fn bad_type(_ctx: &ReducerContext, _a: Test) {}

#[spacetimedb::reducer]
fn bad_return_type(_ctx: &ReducerContext) -> Test {
    Test
}

#[spacetimedb::reducer]
fn lifetime<'a>(_ctx: &ReducerContext, _a: &'a str) {}

#[spacetimedb::reducer]
fn type_param<T>() {}

#[spacetimedb::reducer]
fn const_param<const X: u8>() {}

#[spacetimedb::reducer]
fn missing_ctx(_a: u8) {}

#[spacetimedb::reducer]
fn ctx_by_val(_ctx: ReducerContext, _a: u8) {}

#[spacetimedb::table(name = scheduled_table_missing_rows, scheduled(scheduled_table_missing_rows_reducer))]
struct ScheduledTableMissingRows {
    x: u8,
    y: u8,
}

// #[spacetimedb::reducer]
// fn scheduled_table_missing_rows_reducer(_ctx: &ReducerContext, _: &ScheduledTableMissingRows) {}

#[spacetimedb::table(name = scheduled_table, scheduled(scheduled_table_reducer))]
struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    #[scheduled_at]
    scheduled_at: spacetimedb::ScheduleAt,
    x: u8,
    y: u8,
}

#[spacetimedb::reducer]
fn scheduled_table_reducer(_ctx: &ReducerContext, _x: u8, _y: u8) {}

fn main() {}
