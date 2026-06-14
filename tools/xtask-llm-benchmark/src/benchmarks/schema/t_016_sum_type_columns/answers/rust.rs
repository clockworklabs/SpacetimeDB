use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Rect {
    pub width: i32,
    pub height: i32,
}

#[derive(SpacetimeType, Clone, Debug)]
pub enum Shape {
    Circle(i32),
    Rectangle(Rect),
}

#[table(accessor = drawing)]
pub struct Drawing {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub a: Shape,
    pub b: Shape,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.drawing().insert(Drawing {
        id: 0,
        a: Shape::Circle(10),
        b: Shape::Rectangle(Rect { width: 4, height: 6 }),
    });
}
