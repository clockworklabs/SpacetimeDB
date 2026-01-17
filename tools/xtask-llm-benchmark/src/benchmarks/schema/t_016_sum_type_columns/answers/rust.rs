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

#[table(name = drawing)]
pub struct Drawing {
    #[primary_key]
    pub id: i32,
    pub a: Shape,
    pub b: Shape,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.drawing().insert(Drawing {
        id: 1,
        a: Shape::Circle(10),
        b: Shape::Rectangle(Rect { width: 4, height: 6 }),
    });
}
