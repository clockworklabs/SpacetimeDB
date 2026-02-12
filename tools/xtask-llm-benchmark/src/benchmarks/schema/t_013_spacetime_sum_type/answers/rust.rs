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

#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub value: Shape,
}

#[reducer]
pub fn set_circle(ctx: &ReducerContext, id: i32, radius: i32) {
    ctx.db.result().insert(ResultRow { id, value: Shape::Circle(radius) });
}
