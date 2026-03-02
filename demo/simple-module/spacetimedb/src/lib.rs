use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub created_at_micros: i64,
    pub created_by_hex: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        log::warn!("Ignored add reducer with empty name");
        return;
    }

    let created_by_hex = ctx.sender().to_hex().to_string();
    let created_at_micros = ctx.timestamp.to_micros_since_unix_epoch();

    ctx.db.person().insert(Person {
        id: 0,
        name: trimmed.to_string(),
        created_at_micros,
        created_by_hex,
    });
}

#[spacetimedb::reducer]
pub fn delete_person(ctx: &ReducerContext, id: u64) {
    if ctx.db.person().id().find(id).is_some() {
        ctx.db.person().id().delete(id);
    } else {
        log::warn!("delete_person ignored: id {id} not found");
    }
}
