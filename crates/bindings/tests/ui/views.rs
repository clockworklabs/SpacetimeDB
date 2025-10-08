use spacetimedb::{reducer, table, view, AnonymousViewContext, Identity, ReducerContext, ViewContext};

#[table(name = test)]
struct Test {
    #[unique]
    id: u32,
    #[index(btree)]
    x: u32,
}

#[reducer]
fn view_handle_no_iter(ctx: &ReducerContext) {
    let read_only = ctx.as_read_only();
    // Should not compile: ViewHandle does not expose `iter()`
    for _ in read_only.db.test().iter() {}
}

#[reducer]
fn view_handle_no_count(ctx: &ReducerContext) {
    let read_only = ctx.as_read_only();
    // Should not compile: ViewHandle does not expose `count()`
    let _ = read_only.db.test().count();
}

#[reducer]
fn view_handle_no_insert(ctx: &ReducerContext) {
    let read_only = ctx.as_read_only();
    // Should not compile: ViewHandle does not expose `insert()`
    read_only.db.test().insert(Test { id: 0, x: 0 });
}

#[reducer]
fn view_handle_no_try_insert(ctx: &ReducerContext) {
    let read_only = ctx.as_read_only();
    // Should not compile: ViewHandle does not expose `try_insert()`
    read_only.db.test().try_insert(Test { id: 0, x: 0 });
}

#[reducer]
fn view_handle_no_delete(ctx: &ReducerContext) {
    let read_only = ctx.as_read_only();
    // Should not compile: ViewHandle does not expose `delete()`
    read_only.db.test().delete(Test { id: 0, x: 0 });
}

#[reducer]
fn read_only_unqiue_index_no_delete(ctx: &ReducerContext) {
    let read_only = ctx.as_read_only();
    // Should not compile: unique read-only index does not expose `delete()`
    read_only.db.test().id().delete(&0);
}

#[reducer]
fn read_only_unqiue_index_no_update(ctx: &ReducerContext) {
    let read_only = ctx.as_read_only();
    // Should not compile: unique read-only index does not expose `update()`
    read_only.db.test().id().update(Test { id: 0, x: 0 });
}

#[reducer]
fn read_only_btree_index_no_delete(ctx: &ReducerContext) {
    let read_only = ctx.as_read_only();
    // Should not compile: read-only btree index does not expose `delete()`
    read_only.db.test().x().delete(0u32..);
}

#[table(name = player)]
struct Player {
    #[unique]
    identity: Identity,
}

/// Private views not allowed; must be `#[view(public)]`
#[view]
fn view_def_no_public(_: &ViewContext) -> Vec<Player> {
    vec![]
}

/// A `ViewContext` is required
#[view(public)]
fn view_def_no_context() -> Vec<Player> {
    vec![]
}

/// A `ViewContext` is required
#[view(public)]
fn view_def_wrong_context_1(_: &ReducerContext) -> Vec<Player> {
    vec![]
}

/// A `ViewContext` is required
#[view(public)]
fn view_def_wrong_context_2(_: &AnonymousViewContext) -> Vec<Player> {
    vec![]
}

/// An `AnonymousViewContext` is required
#[view(public, anonymous)]
fn anonymous_view_def_no_context() -> Vec<Player> {
    vec![]
}

/// An `AnonymousViewContext` is required
#[view(public, anonymous)]
fn anonymous_view_def_wrong_context_1(_: &ReducerContext) -> Vec<Player> {
    vec![]
}

/// An `AnonymousViewContext` is required
#[view(public, anonymous)]
fn anonymous_view_def_wrong_context_2(_: &ViewContext) -> Vec<Player> {
    vec![]
}

/// Must return `Vec<T>` where `T` is a SpacetimeType
#[view(public)]
fn view_def_no_return(_: &ViewContext) {}

/// Must return `Vec<T>` where `T` is a SpacetimeType
#[view(public)]
fn view_def_wrong_return(_: &ViewContext) -> Option<Player> {
    None
}

fn main() {}
