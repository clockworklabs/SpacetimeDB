use spacetimedb::{spacetimedb, ReducerContext, Identity, SpacetimeType, schedule};
use rand::Rng;
use std::time::Duration;

#[spacetimedb(table)]
pub struct Entity {
    #[autoinc]
    #[primarykey]
    pub id: u32,
    pub position: Vector2,
    pub radius: u32,
}

#[spacetimedb(table)]
pub struct Circle {
    #[primarykey]
    pub circle_id: Identity,
    #[unique]
    pub entity_id: u32,
    #[unique]
    pub name: String,
    pub direction: Vector2,
    pub magnitude: f32,
}

#[spacetimedb(table)]
pub struct LoggedOutCircle {
    #[unique]
    circle_id: Identity,
    circle: Circle,
    entity: Entity,
}

#[spacetimedb(table)]
pub struct Food {
    #[primarykey]
    pub entity_id: u32,
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    // Function to normalize the vector
    fn normalize(&self) -> Vector2 {
        let mag = (self.x * self.x + self.y * self.y).sqrt();
        if mag != 0.0 {
            Vector2 { x: self.x / mag, y: self.y / mag, }
        } else {
            Vector2 { x: 0.0, y: 0.0 }
        }
    }
}

const WORLD_SIZE: f64 = 1000.0;
const START_PLAYER_RADIUS: u32 = 5;

#[spacetimedb(init)]
pub fn init() -> Result<(), String> {
    log::info!("Initializing...");
    spawn_food()?;
    move_all_players()?;
    Ok(())
}

#[spacetimedb(disconnect)]
pub fn disconnect(ctx: ReducerContext) -> Result<(), String> {
    let circle = Circle::filter_by_circle_id(&ctx.sender).ok_or("Circle not found")?;
    let entity = Entity::filter_by_id(&circle.entity_id).ok_or("Entity not found")?;
    Entity::delete_by_id(&entity.id);
    LoggedOutCircle::insert(LoggedOutCircle {
        circle_id: circle.circle_id,
        circle,
        entity,
    })?;
    Circle::delete_by_circle_id(&ctx.sender);
    Ok(())
}

#[spacetimedb(connect)]
pub fn connect(ctx: ReducerContext) -> Result<(), String> {
    let logged_out_circle = LoggedOutCircle::filter_by_circle_id(&ctx.sender).ok_or("Logged out circle not found")?;
    Circle::insert(logged_out_circle.circle)?;
    Entity::insert(logged_out_circle.entity)?;
    LoggedOutCircle::delete_by_circle_id(&ctx.sender);
    Ok(())
}

#[spacetimedb(reducer)]
pub fn create_player(ctx: ReducerContext, name: String) -> Result<(), String> {
    let mut rng = rand::thread_rng();
    let x = rng.gen_range(START_PLAYER_RADIUS as f32..(WORLD_SIZE as f32 - START_PLAYER_RADIUS as f32));
    let y = rng.gen_range(START_PLAYER_RADIUS as f32..(WORLD_SIZE as f32 - START_PLAYER_RADIUS as f32));
    let entity = Entity::insert(Entity {
        id: 0,
        position: Vector2 { x, y },
        radius: START_PLAYER_RADIUS,
    })?;

    Circle::insert(Circle {
        entity_id: entity.id,
        circle_id: ctx.sender,
        name,
        direction: Vector2 { x: 0.0, y: 1.0 },
        magnitude: 0.0,
    })?;

    Ok(())
}

#[spacetimedb(reducer)]
pub fn update_player_input(ctx: ReducerContext, direction: Vector2, magnitude: f32) -> Result<(), String> {
    let mut circle = Circle::filter_by_circle_id(&ctx.sender).ok_or("Circle not found")?;
    circle.direction = direction.normalize();
    circle.magnitude = magnitude.clamp(0.0, 1.0);
    Circle::update_by_circle_id(&ctx.sender, circle);
    Ok(())
}

fn is_overlapping(entity1: &Entity, entity2: &Entity) -> bool {
    let distance = ((entity1.position.x - entity2.position.x).powi(2) + (entity1.position.y - entity2.position.y).powi(2)).sqrt();
    distance < (entity1.radius + entity2.radius) as f32
}

#[spacetimedb(reducer)]
pub fn move_all_players() -> Result<(), String> {
    for circle in Circle::iter() {
        let mut circle_entity = Entity::filter_by_id(&circle.entity_id).ok_or("Entity not found")?;
        let x = circle_entity.position.x + circle.direction.x * circle.magnitude;
        let y = circle_entity.position.y + circle.direction.y * circle.magnitude;
        circle_entity.position.x = x.clamp(0.0, WORLD_SIZE as f32);
        circle_entity.position.y = y.clamp(0.0, WORLD_SIZE as f32);
        log::info!("Move player: {} {}", x, y);

        // Check to see if we're overlapping with food
        for food in Food::iter() {
            let food_entity = Entity::filter_by_id(&food.entity_id).ok_or("Entity not found")?;
            if is_overlapping(&circle_entity, &food_entity) {
                // We're overlapping with food, so eat it
                Entity::delete_by_id(&food.entity_id);
                Food::delete_by_entity_id(&food.entity_id);
                circle_entity.radius += food_entity.radius;
            }
        }

        // Check to see if we're overlapping with another player
        // for circle in Circle::iter() {
        //     if circle.circle_id == circle.circle_id {
        //         continue;
        //     }
        //     let other_entity = Entity::filter_by_id(&circle.entity_id).ok_or("Entity not found")?;
        //     if is_overlapping(&circle_entity, &other_entity) && circle_entity.size - 5 > other_entity.size {
        //         // We're overlapping with another player, so eat them
        //         Entity::delete_by_id(&other_entity.id);
        //         Circle::delete_by_circle_id(&circle.circle_id);
        //         circle_entity.size += other_entity.size;
        //     }
        // }

        Entity::update_by_id(&circle_entity.id.clone(), circle_entity);
    }

    schedule!(Duration::from_millis(50), move_all_players());
    Ok(())
}

#[spacetimedb(reducer)]
pub fn spawn_food() -> Result<(), String> {
    // Is there too much food already? Are there no players yet?
    if Food::iter().count() > 200
    // || Circle::iter().count() == 0
    {
        schedule!(Duration::from_millis(100), spawn_food());
        return Ok(());
    }

    let mut rng = rand::thread_rng();
    let x = rng.gen_range(0.0..WORLD_SIZE) as f32;
    let y = rng.gen_range(0.0..WORLD_SIZE) as f32;
    let entity = Entity::insert(Entity {
        id: 0,
        position: Vector2 { x, y },
        radius: 1,
    })?;
    Food::insert(Food { entity_id: entity.id })?;
    log::info!("Spawned food! {}", entity.id);

    spawn_food().unwrap();
    Ok(())
}
