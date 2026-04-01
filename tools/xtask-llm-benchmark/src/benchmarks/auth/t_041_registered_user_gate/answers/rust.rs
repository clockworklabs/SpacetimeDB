use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(accessor = user)]
pub struct User {
    #[primary_key]
    pub identity: Identity,
    pub name: String,
}

#[table(accessor = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub sender: Identity,
    pub text: String,
}

#[reducer]
pub fn register(ctx: &ReducerContext, name: String) {
    if ctx.db.user().identity().find(ctx.sender()).is_some() {
        panic!("already registered");
    }
    ctx.db.user().insert(User {
        identity: ctx.sender(),
        name,
    });
}

#[reducer]
pub fn post_message(ctx: &ReducerContext, text: String) {
    if ctx.db.user().identity().find(ctx.sender()).is_none() {
        panic!("not registered");
    }
    ctx.db.message().insert(Message {
        id: 0,
        sender: ctx.sender(),
        text,
    });
}
