use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = secret, private)]
pub struct Secret {
    answer: u8,
}

#[spacetimedb::table(accessor = common_knowledge, public)]
pub struct CommonKnowledge {
    thing: String,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.secret().insert(Secret { answer: 42 });
}

#[spacetimedb::reducer]
pub fn do_thing(ctx: &ReducerContext, thing: String) {
    ctx.db.secret().insert(Secret { answer: 20 });
    ctx.db.common_knowledge().insert(CommonKnowledge { thing });
}
