use spacetimedb::{ReducerContext, SpacetimeType, Table};

const TARGET_FOOD_COUNT: usize = 600;

#[spacetimedb::table(name = entity, public)]
pub struct Entity {
    #[auto_inc]
    #[primary_key]
    pub id: u32,
    pub position: Vector2,
}

#[spacetimedb::table(name = food, public)]
pub struct Food {
    #[primary_key]
    pub entity_id: u32,
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

#[spacetimedb::reducer]
pub fn spawn_food(ctx: &ReducerContext) -> Result<(), String> {
    let food_count = ctx.db.food().count();

    if food_count < TARGET_FOOD_COUNT as u64 {
        let x = 1.0;
        let y = 1.0;
        ctx.db.entity().try_insert(Entity {
            id: 0,
            position: Vector2 { x, y },
        })?;
    }

    Ok(())
}
