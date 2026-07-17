use spacetimedb::{table, view, AnonymousViewContext};
#[table(accessor = source_row, public)]
pub struct SourceRow { #[primary_key] pub id: u64, pub value: String, #[index(btree)] pub visible: bool }
#[view(accessor = source_view, public, primary_key = id)]
pub fn source_view(ctx: &AnonymousViewContext) -> Vec<SourceRow> { ctx.db.source_row().visible().filter(true).collect() }
