use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(
    name = logs,
    index(name = by_user_day, btree(columns = [user_id, day]))
)]
pub struct Log {
    #[primary_key]
    pub id: i32,
    pub user_id: i32,
    pub day: i32,
    pub message: String,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.logs().insert(Log { id: 1, user_id: 7, day: 1, message: "a".into() });
    ctx.db.logs().insert(Log { id: 2, user_id: 7, day: 2, message: "b".into() });
    ctx.db.logs().insert(Log { id: 3, user_id: 9, day: 1, message: "c".into() });
}
