use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = my_table, public)]
pub struct MyTable {
    x: String,
}

#[spacetimedb::reducer]
fn do_schedule(_ctx: &ReducerContext) {
    spacetimedb::volatile_nonatomic_schedule_immediate!(do_insert("hello".to_owned()));
}

#[spacetimedb::reducer]
fn do_insert(ctx: &ReducerContext, x: String) {
    ctx.db.my_table().insert(MyTable { x });
}
