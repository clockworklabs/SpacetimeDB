use spacetimedb::{reducer, table, view, Identity, ReducerContext, SpacetimeType, Table, ViewContext};

#[table(accessor = secret_note)]
pub struct SecretNote {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub title: String,
    pub secret_body: String,
}

#[derive(SpacetimeType)]
pub struct SafeNote {
    pub id: u64,
    pub title: String,
}

#[reducer]
pub fn seed_private_note(ctx: &ReducerContext) {
    ctx.db.secret_note().insert(SecretNote {
        id: 1,
        owner: ctx.sender(),
        title: "Visible title".into(),
        secret_body: "never expose this".into(),
    });
}

#[view(accessor = my_safe_note, public)]
pub fn my_safe_note(ctx: &ViewContext) -> Vec<SafeNote> {
    ctx.db
        .secret_note()
        .owner()
        .filter(ctx.sender())
        .map(|note| SafeNote {
            id: note.id,
            title: note.title,
        })
        .collect()
}
