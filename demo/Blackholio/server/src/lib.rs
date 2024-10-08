use spacetimedb::{ReducerContext, SpacetimeType, Table};

#[spacetimedb::table(name = entity, public)]
pub struct Entity {
    pub position: Vector2,
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

#[spacetimedb::reducer]
pub fn spawn_food(ctx: &ReducerContext) -> Result<(), String> {
    let count = ctx.db.entity().count();

    if count < 600 as u64 {
        ctx.db.entity().try_insert(Entity {
            position: Vector2 { x: 0.0, y: 0.0 },
        })?;
    }

    Ok(())
}
