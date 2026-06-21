use spacetimedb::{reducer, table, view, ReducerContext, SpacetimeType, Table, ViewContext};

#[derive(Clone)]
#[table(accessor = old, public)]
pub struct OldSource {
    #[primary_key]
    id: u64,
}

#[derive(Clone)]
#[table(accessor = new, public)]
pub struct NewSource {
    #[primary_key]
    id: u64,
}

#[derive(Clone, SpacetimeType)]
pub struct Row {
    id: u64,
}

impl From<OldSource> for Row {
    fn from(row: OldSource) -> Self {
        Row { id: row.id }
    }
}

impl From<NewSource> for Row {
    fn from(row: NewSource) -> Self {
        Row { id: row.id }
    }
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.old().insert(OldSource { id: 0 });
    ctx.db.new().insert(NewSource { id: 0 });
}

#[reducer]
pub fn touch_old(ctx: &ReducerContext) {
    ctx.db.old().id().delete(&0);
}

const SWITCHED_LOG: &str = "Calling or updating view `switched`";

#[view(accessor = switched, public)]
pub fn switched(ctx: &ViewContext) -> Option<Row> {
    log::info!("{SWITCHED_LOG}");
    ctx.db.old().id().find(&0).map(|row| row.into())
}
