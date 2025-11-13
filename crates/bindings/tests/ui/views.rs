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

struct NotSpacetimeType {}

/// Private views not allowed; must be `#[view(public, ...)]`
#[view(name = view_def_no_public)]
fn view_def_no_public(_: &ViewContext) -> Vec<Player> {
    vec![]
}

/// Duplicate `public`
#[view(name = view_def_dup_public, public, public)]
fn view_def_dup_public() -> Vec<Player> {
    vec![]
}

/// Duplicate `name`
#[view(name = view_def_dup_name, name = view_def_dup_name, public)]
fn view_def_dup_name() -> Vec<Player> {
    vec![]
}

/// Unsupported attribute arg
#[view(name = view_def_unsupported_arg, public, anonymous)]
fn view_def_unsupported_arg() -> Vec<Player> {
    vec![]
}

/// A `ViewContext` is required
#[view(name = view_def_no_context, public)]
fn view_def_no_context() -> Vec<Player> {
    vec![]
}

/// A `ViewContext` is required
#[view(name = view_def_wrong_context, public)]
fn view_def_wrong_context(_: &ReducerContext) -> Vec<Player> {
    vec![]
}

/// Must pass the `ViewContext` by ref
#[view(name = view_def_pass_context_by_value, public)]
fn view_def_pass_context_by_value(_: ViewContext) -> Vec<Player> {
    vec![]
}

/// The view context must be the first parameter
#[view(name = view_def_wrong_context_position, public)]
fn view_def_wrong_context_position(_: &u32, _: &ViewContext) -> Vec<Player> {
    vec![]
}

/// Must return `Vec<T>` or `Option<T>` where `T` is a SpacetimeType
#[view(name = view_def_no_return, public)]
fn view_def_no_return(_: &ViewContext) {}

/// Must return `Vec<T>` or `Option<T>` where `T` is a SpacetimeType
#[view(name = view_def_wrong_return, public)]
fn view_def_wrong_return(_: &ViewContext) -> Player {
    Player {
        identity: Identity::ZERO,
    }
}

/// Must return `Vec<T>` or `Option<T>` where `T` is a SpacetimeType
#[view(name = view_def_returns_not_a_spacetime_type, public)]
fn view_def_returns_not_a_spacetime_type(_: &AnonymousViewContext) -> Option<NotSpacetimeType> {
    None
}

/// Cannot use a view as a scheduled function
#[spacetimedb::table(name = scheduled_table, scheduled(scheduled_table_view))]
struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    x: u8,
    y: u8,
}

/// Cannot use a view as a scheduled function
#[view(name = scheduled_table_view, public)]
fn scheduled_table_view(_: &ViewContext, _args: ScheduledTable) -> Vec<Player> {
    vec![]
}

fn main() {}
