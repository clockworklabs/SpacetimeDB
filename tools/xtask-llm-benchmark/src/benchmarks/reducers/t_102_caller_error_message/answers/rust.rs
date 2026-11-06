use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = item, public)]
pub struct Item {
    #[primary_key]
    pub id: u64,
    pub name: String,
}

#[reducer]
pub fn add_item(ctx: &ReducerContext, id: u64, name: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("name required".to_string());
    }
    ctx.db.item().insert(Item { id, name });
    Ok(())
}
