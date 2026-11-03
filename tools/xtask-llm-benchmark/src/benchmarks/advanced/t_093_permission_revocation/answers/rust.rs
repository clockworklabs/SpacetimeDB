use spacetimedb::{reducer, table, ReducerContext, Table};
use spacetimedb::{view, SpacetimeType, ViewContext};
#[table(accessor = private_document)]
pub struct PrivateDocument {
    #[primary_key]
    pub id: u64,
    pub owner: spacetimedb::Identity,
    pub title: String,
    pub secret_body: String,
}

#[table(accessor = read_access)]
pub struct ReadAccess {
    #[primary_key]
    pub reader: spacetimedb::Identity,
    pub enabled: bool,
}

#[reducer]
pub fn set_document(ctx: &ReducerContext, title: String, secret_body: String) -> Result<(), String> {
    if let Some(mut row) = ctx.db.private_document().id().find(1) {
        if row.owner != ctx.sender() {
            return Err("owner only".into());
        }
        row.title = title;
        row.secret_body = secret_body;
        ctx.db.private_document().id().update(row);
    } else {
        ctx.db.private_document().insert(PrivateDocument {
            id: 1,
            owner: ctx.sender(),
            title,
            secret_body,
        });
    }
    Ok(())
}

#[reducer]
pub fn set_access(ctx: &ReducerContext, reader: spacetimedb::Identity, enabled: bool) -> Result<(), String> {
    let row = ctx.db.private_document().id().find(1).ok_or("owner only")?;
    if row.owner != ctx.sender() {
        return Err("owner only".into());
    }
    let access = ReadAccess { reader, enabled };
    if ctx.db.read_access().reader().find(reader).is_some() {
        ctx.db.read_access().reader().update(access);
    } else {
        ctx.db.read_access().insert(access);
    }
    Ok(())
}

#[derive(SpacetimeType)]
pub struct SafeDocument {
    pub id: u64,
    pub title: String,
}
#[view(accessor = visible_document, public)]
pub fn visible_document(ctx: &ViewContext) -> Vec<SafeDocument> {
    let Some(row) = ctx.db.private_document().id().find(1) else {
        return vec![];
    };
    let allowed = ctx
        .db
        .read_access()
        .reader()
        .find(ctx.sender())
        .is_some_and(|r| r.enabled);
    if row.owner == ctx.sender() || allowed {
        vec![SafeDocument {
            id: row.id,
            title: row.title,
        }]
    } else {
        vec![]
    }
}
