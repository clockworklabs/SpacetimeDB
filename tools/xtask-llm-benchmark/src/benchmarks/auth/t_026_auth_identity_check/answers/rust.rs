use spacetimedb::Identity;
use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub text: String,
}

#[reducer]
pub fn create_message(ctx: &ReducerContext, text: String) {
    ctx.db.message().insert(Message {
        id: 0,
        owner: ctx.sender(),
        text,
    });
}

#[reducer]
pub fn delete_message(ctx: &ReducerContext, id: u64) {
    let msg = ctx.db.message().id().find(id).expect("not found");
    if msg.owner != ctx.sender() {
        panic!("unauthorized");
    }
    ctx.db.message().id().delete(id);
}
