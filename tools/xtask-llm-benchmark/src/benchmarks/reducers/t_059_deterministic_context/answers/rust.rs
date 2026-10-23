use spacetimedb::{reducer, table, ReducerContext, Table, Timestamp};

#[table(accessor = generated_value, public)]
pub struct GeneratedValue {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub created_at: Timestamp,
    pub random_value: u64,
}

#[reducer]
pub fn generate(ctx: &ReducerContext) {
    let random_value: u64 = ctx.random();
    ctx.db.generated_value().insert(GeneratedValue {
        id: 0,
        created_at: ctx.timestamp,
        random_value: random_value.max(1),
    });
}
