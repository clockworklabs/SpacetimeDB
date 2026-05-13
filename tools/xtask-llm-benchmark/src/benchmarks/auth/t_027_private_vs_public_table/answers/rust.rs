use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = user_internal)]
pub struct UserInternal {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub email: String,
    pub password_hash: String,
}

#[table(accessor = user_public, public)]
pub struct UserPublic {
    #[primary_key]
    pub id: u64,
    pub name: String,
}

#[reducer]
pub fn register_user(ctx: &ReducerContext, name: String, email: String, password_hash: String) {
    let internal = ctx.db.user_internal().insert(UserInternal {
        id: 0,
        name: name.clone(),
        email,
        password_hash,
    });
    ctx.db.user_public().insert(UserPublic { id: internal.id, name });
}
