use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = author)]
pub struct Author {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
}

#[table(accessor = post)]
pub struct Post {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub author_id: u64,
    pub title: String,
}

#[reducer]
pub fn delete_author(ctx: &ReducerContext, author_id: u64) {
    // Delete all posts by this author
    for p in ctx.db.post().author_id().filter(&author_id) {
        ctx.db.post().id().delete(p.id);
    }
    // Delete the author
    ctx.db.author().id().delete(author_id);
}
