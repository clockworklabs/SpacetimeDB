use spacetimedb::{Identity, ReducerContext, Table};

#[spacetimedb::table(accessor = doc, public)]
pub struct Doc {
    #[index(btree)]
    id: u64,
    #[index(btree)]
    scope: u64,
}

#[spacetimedb::table(accessor = member, public)]
pub struct Member {
    #[index(btree)]
    scope: u64,
    #[index(btree)]
    identity: Identity,
}

#[spacetimedb::client_visibility_filter]
const DOC_FILTER: spacetimedb::Filter = spacetimedb::Filter::Sql(
    "SELECT doc.* FROM doc JOIN member ON doc.scope = member.scope WHERE member.identity = :sender",
);

#[spacetimedb::client_visibility_filter]
const MEMBER_FILTER: spacetimedb::Filter = spacetimedb::Filter::Sql("SELECT * FROM member WHERE identity = :sender");

#[spacetimedb::reducer]
pub fn join(ctx: &ReducerContext, scope: u64) {
    ctx.db.member().insert(Member {
        scope,
        identity: ctx.sender(),
    });
}

#[spacetimedb::reducer]
pub fn add_doc(ctx: &ReducerContext, id: u64, scope: u64) {
    ctx.db.doc().insert(Doc { id, scope });
}
