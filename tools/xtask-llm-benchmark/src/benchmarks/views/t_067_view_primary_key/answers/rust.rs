use spacetimedb::{table, view, AnonymousViewContext, ReducerContext, Table};

#[table(accessor = source_row, public)]
pub struct SourceRow {
    #[primary_key]
    pub id: u64,
    pub value: String,
    #[index(btree)]
    pub visible: bool,
}

#[spacetimedb::reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.source_row().insert(SourceRow {
        id: 1,
        value: "shown".into(),
        visible: true,
    });
    ctx.db.source_row().insert(SourceRow {
        id: 2,
        value: "hidden".into(),
        visible: false,
    });
}

#[view(accessor = source_view, public, primary_key = id)]
pub fn source_view(ctx: &AnonymousViewContext) -> Vec<SourceRow> {
    ctx.db.source_row().visible().filter(true).collect()
}
