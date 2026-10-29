use spacetimedb::{reducer, table, view, Identity, ReducerContext, SpacetimeType, Table, ViewContext};
#[table(accessor = private_note)]
pub struct PrivateNote {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub tag: String,
    pub text: String,
}
#[derive(SpacetimeType)]
pub struct NoteResult {
    pub id: u64,
    pub text: String,
}
#[reducer]
pub fn put_note(ctx: &ReducerContext, id: u64, tag: String, text: String) -> Result<(), String> {
    let row = PrivateNote {
        id,
        owner: ctx.sender(),
        tag,
        text,
    };
    if let Some(old) = ctx.db.private_note().id().find(id) {
        if old.owner != ctx.sender() {
            return Err("owner only".into());
        }
        ctx.db.private_note().id().update(row);
    } else {
        ctx.db.private_note().insert(row);
    }
    Ok(())
}
#[reducer]
pub fn remove_note(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    if let Some(old) = ctx.db.private_note().id().find(id) {
        if old.owner != ctx.sender() {
            return Err("owner only".into());
        }
        ctx.db.private_note().id().delete(id);
    }
    Ok(())
}
#[view(accessor = notes_for_tag, public)]
pub fn notes_for_tag(ctx: &ViewContext, tag: String) -> Vec<NoteResult> {
    ctx.db
        .private_note()
        .owner()
        .filter(ctx.sender())
        .filter(|row| row.tag == tag)
        .map(|row| NoteResult {
            id: row.id,
            text: row.text,
        })
        .collect()
}
