//! STDB module used for benchmarks based on "realistic" workloads we are focusing in improving.
use crate::Load;
use spacetimedb::{log, ReducerContext, SpacetimeType, Table, Timestamp};
use std::hint::black_box;

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

// ---------- schemas ----------

#[spacetimedb::table(name = entity)]
pub struct Entity {
    #[auto_inc]
    #[primary_key]
    pub id: u32,
    pub position: Vector2,
    pub mass: u32,
}

impl Entity {
    pub fn new(id: u32, x: f32, y: f32, mass: u32) -> Self {
        Self {
            id,
            mass,
            position: Vector2 { x, y },
        }
    }
}

#[spacetimedb::table(name = circle)]
pub struct Circle {
    #[primary_key]
    pub entity_id: u32,
    #[index(btree)]
    pub player_id: u32,
    pub direction: Vector2,
    pub magnitude: f32,
    pub last_split_time: Timestamp,
}

impl Circle {
    pub fn new(entity_id: u32, player_id: u32, x: f32, y: f32, magnitude: f32, last_split_time: Timestamp) -> Self {
        Self {
            entity_id,
            player_id,
            direction: Vector2 { x, y },
            magnitude,
            last_split_time,
        }
    }
}

#[spacetimedb::table(name = food)]
pub struct Food {
    #[primary_key]
    pub entity_id: u32,
}

impl Food {
    pub fn new(entity_id: u32) -> Self {
        Self { entity_id }
    }
}

fn mass_to_radius(mass: u32) -> f32 {
    (mass as f32).sqrt()
}

fn is_overlapping(entity1: &Entity, entity2: &Entity) -> bool {
    let entity1_radius = mass_to_radius(entity1.mass);
    let entity2_radius = mass_to_radius(entity2.mass);
    let distance =
        ((entity1.position.x - entity2.position.x).powi(2) + (entity1.position.y - entity2.position.y).powi(2)).sqrt();
    distance < entity1_radius.max(entity2_radius)
}

// ---------- insert bulk ----------
#[spacetimedb::reducer]
pub fn insert_bulk_entity(ctx: &ReducerContext, count: u32) {
    for id in 0..count {
        ctx.db
            .entity()
            .insert(Entity::new(0, id as f32, (id + 5) as f32, id * 5));
    }
    log::info!("INSERT ENTITY: {count}");
}

#[spacetimedb::reducer]
pub fn insert_bulk_circle(ctx: &ReducerContext, count: u32) {
    for id in 0..count {
        ctx.db.circle().insert(Circle::new(
            id,
            id,
            id as f32,
            (id + 5) as f32,
            (id * 5) as f32,
            ctx.timestamp,
        ));
    }
    log::info!("INSERT CIRCLE: {count}");
}

#[spacetimedb::reducer]
pub fn insert_bulk_food(ctx: &ReducerContext, count: u32) {
    for id in 1..=count {
        ctx.db.food().insert(Food::new(id));
    }
    log::info!("INSERT FOOD: {count}");
}

// Simulate
// ```
// SELECT * FROM Circle, Entity, Food
// ```
#[spacetimedb::reducer]
pub fn cross_join_all(ctx: &ReducerContext, expected: u32) {
    let mut count = 0;
    for _circle in ctx.db.circle().iter() {
        for _entity in ctx.db.entity().iter() {
            for _food in ctx.db.food().iter() {
                count += 1;
            }
        }
    }

    log::info!("CROSS JOIN ALL: {expected}, processed: {count}");
}

// Simulate
// ```
// SELECT * FROM Circle JOIN ENTITY USING(entity_id), Food JOIN ENTITY USING(entity_id)
// ```
#[spacetimedb::reducer]
pub fn cross_join_circle_food(ctx: &ReducerContext, expected: u32) {
    let mut count = 0;
    for circle in ctx.db.circle().iter() {
        let Some(circle_entity) = ctx.db.entity().id().find(circle.entity_id) else {
            continue;
        };

        for food in ctx.db.food().iter() {
            count += 1;
            let food_entity = ctx
                .db
                .entity()
                .id()
                .find(food.entity_id)
                .unwrap_or_else(|| panic!("Entity not found: {}", food.entity_id));
            black_box(is_overlapping(&circle_entity, &food_entity));
        }
    }

    log::info!("CROSS JOIN CIRCLE FOOD: {expected}, processed: {count}");
}

#[spacetimedb::reducer]
pub fn init_game_circles(ctx: &ReducerContext, initial_load: u32) {
    let load = Load::new(initial_load);

    insert_bulk_food(ctx, load.initial_load);
    insert_bulk_entity(ctx, load.initial_load);
    insert_bulk_circle(ctx, load.small_table);
}

#[spacetimedb::reducer]
pub fn run_game_circles(ctx: &ReducerContext, initial_load: u32) {
    let load = Load::new(initial_load);

    cross_join_circle_food(ctx, initial_load * load.small_table);
    cross_join_all(ctx, initial_load * initial_load * load.small_table);
}
