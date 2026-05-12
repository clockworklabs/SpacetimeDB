use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = primitive)]
pub struct Primitive {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub count: i32,
    pub total: i64,
    pub price: f32,
    pub ratio: f64,
    pub active: bool,
    pub name: String,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.primitive().insert(Primitive {
        id: 0,
        count: 2,
        total: 3_000_000_000,
        price: 1.5,
        ratio: 2.25,
        active: true,
        name: "Alice".into(),
    });
}
