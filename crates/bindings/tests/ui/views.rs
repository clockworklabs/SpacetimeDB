use spacetimedb::{reducer, table, ReducerContext};

#[table(name = test)]
struct Test {
    #[unique]
    id: u32,
    #[index(btree)]
    x: u32,
}

#[reducer]
fn view_handle_no_iter(ctx: &ReducerContext) {
    let read_only = ctx.as_view();
    // Should not compile: ViewHandle does not expose `iter()`
    for _ in read_only.db.test().iter() {}
}

#[reducer]
fn view_handle_no_count(ctx: &ReducerContext) {
    let read_only = ctx.as_view();
    // Should not compile: ViewHandle does not expose `count()`
    let _ = read_only.db.test().count();
}

#[reducer]
fn view_handle_no_insert(ctx: &ReducerContext) {
    let read_only = ctx.as_view();
    // Should not compile: ViewHandle does not expose `insert()`
    read_only.db.test().insert(Test { id: 0, x: 0 });
}

#[reducer]
fn view_handle_no_try_insert(ctx: &ReducerContext) {
    let read_only = ctx.as_view();
    // Should not compile: ViewHandle does not expose `try_insert()`
    read_only.db.test().try_insert(Test { id: 0, x: 0 });
}

#[reducer]
fn view_handle_no_delete(ctx: &ReducerContext) {
    let read_only = ctx.as_view();
    // Should not compile: ViewHandle does not expose `delete()`
    read_only.db.test().delete(Test { id: 0, x: 0 });
}

#[reducer]
fn read_only_unqiue_index_no_delete(ctx: &ReducerContext) {
    let read_only = ctx.as_view();
    // Should not compile: unique read-only index does not expose `delete()`
    read_only.db.test().id().delete(&0);
}

#[reducer]
fn read_only_unqiue_index_no_update(ctx: &ReducerContext) {
    let read_only = ctx.as_view();
    // Should not compile: unique read-only index does not expose `update()`
    read_only.db.test().id().update(Test { id: 0, x: 0 });
}

#[reducer]
fn read_only_btree_index_no_delete(ctx: &ReducerContext) {
    let read_only = ctx.as_view();
    // Should not compile: read-only btree index does not expose `delete()`
    read_only.db.test().x().delete(0u32..);
}

fn main() {}
