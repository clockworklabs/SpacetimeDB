use spacetimedb::{spacetimedb, ReducerContext, Identity, SpacetimeType, schedule};
use rand::Rng;
use std::time::Duration;

// TODO:
// - Remove players when they are eaten on the client + death + respawn screen
// - Player splitting + increased area of view
// - Viruses
// - Ejecting mass
// - Leaderboard
// - Overlap amount should be more significant in order to eat

#[spacetimedb(table)]
pub struct Config {
    #[primarykey]
    pub id: u32,
    pub world_size: u64,
}

#[spacetimedb(table)]
pub struct Entity {
    #[autoinc]
    #[primarykey]
    pub id: u32,
    pub position: Vector2,
    pub mass: u32,
}

#[spacetimedb(table)]
pub struct Circle {
    #[primarykey]
    pub entity_id: u32,
    pub direction: Vector2,
    pub magnitude: f32,
}

#[spacetimedb(table)]
pub struct Player {
    #[primarykey]
    player_id: Identity,
    #[unique]
    entity_id: u32,
    name: String,
}

#[spacetimedb(table)]
pub struct LoggedOutCircle {
    #[unique]
    player_id: Identity,
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

const START_PLAYER_MASS: u32 = 12;
const START_PLAYER_SPEED: u32 = 10;
const FOOD_MASS_MIN: u32 = 2;
const FOOD_MASS_MAX: u32 = 4;

#[spacetimedb(init)]
pub fn init() -> Result<(), String> {
    log::info!("Initializing...");
    Config::insert(Config { id: 0, world_size: 1000 })?;
    spawn_food()?;
    move_all_players()?;
    circle_decay()?;
    Ok(())
}

#[spacetimedb(disconnect)]
pub fn disconnect(ctx: ReducerContext) -> Result<(), String> {
    let player = Player::filter_by_player_id(&ctx.sender).ok_or("Player not found")?;
    let circle = Circle::filter_by_entity_id(&player.entity_id).ok_or("Could not find circle")?;
    let entity = Entity::filter_by_id(&player.entity_id).ok_or("Could not find circle")?;
    Entity::delete_by_id(&player.entity_id);
    Circle::delete_by_entity_id(&player.entity_id);
    LoggedOutCircle::insert(LoggedOutCircle {
        player_id: player.player_id,
        circle,
        entity,
    })?;
    Ok(())
}

#[spacetimedb(connect)]
pub fn connect(ctx: ReducerContext) -> Result<(), String> {
    let logged_out_circle = LoggedOutCircle::filter_by_player_id(&ctx.sender).ok_or("Logged out circle not found")?;
    Circle::insert(logged_out_circle.circle)?;
    Entity::insert(logged_out_circle.entity)?;
    LoggedOutCircle::delete_by_player_id(&ctx.sender);
    Ok(())
}

#[spacetimedb(reducer)]
pub fn create_player(ctx: ReducerContext, name: String) -> Result<(), String> {
    let entity = spawn_circle(None)?;
    Player::insert(Player {
        player_id: ctx.sender,
        entity_id: entity.id,
        name,
    })?;

    Ok(())
}

#[spacetimedb(reducer)]
pub fn respawn(ctx: ReducerContext) -> Result<(), String> {
    let player = Player::filter_by_player_id(&ctx.sender).ok_or("No such player found")?;
    spawn_circle(Some(player.entity_id))?;
    Ok(())
}

fn spawn_circle(entity_id: Option<u32>) -> Result<Entity, String> {
    let mut rng = rand::thread_rng();
    let world_size = Config::filter_by_id(&0).ok_or("Config not found")?.world_size;
    let player_start_radius = mass_to_radius(START_PLAYER_MASS);
    let x = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    let y = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    let entity = Entity::insert(Entity {
        id: entity_id.unwrap_or(0),
        position: Vector2 { x, y },
        mass: START_PLAYER_MASS,
    })?;

    Circle::insert(Circle {
        entity_id: entity.id,
        direction: Vector2 { x: 0.0, y: 1.0 },
        magnitude: 0.0,
    })?;
    Ok(entity)
}

#[spacetimedb(reducer)]
pub fn update_player_input(ctx: ReducerContext,
                           direction: Vector2, magnitude: f32) -> Result<(), String> {
    let player = Player::filter_by_player_id(&ctx.sender).ok_or("Player not found")?;
    let mut circle = Circle::filter_by_entity_id(&player.entity_id).ok_or("Circle not found")?;
    circle.direction = direction.normalize();
    circle.magnitude = magnitude.clamp(0.0, 1.0);
    Circle::update_by_entity_id(&player.entity_id, circle);
    Ok(())
}

fn is_overlapping(entity1: &Entity, entity2: &Entity) -> bool {
    let entity1_radius = mass_to_radius(entity1.mass);
    let entity2_radius = mass_to_radius(entity2.mass);
    let distance = ((entity1.position.x - entity2.position.x).powi(2) + (entity1.position.y - entity2.position.y).powi(2)).sqrt();
    distance < (entity1_radius + entity2_radius)
}

fn mass_to_radius(mass: u32) -> f32 {
    (mass as f32).sqrt()
}

fn mass_to_max_move_speed(mass: u32) -> f32 {
    2.0 * START_PLAYER_SPEED as f32 / (1.0 + (mass as f32 / START_PLAYER_MASS as f32).sqrt())
}

#[spacetimedb(reducer)]
pub fn move_all_players() -> Result<(), String> {
    let world_size = Config::filter_by_id(&0).ok_or("Config not found")?.world_size;
    for circle in Circle::iter() {
        let mut circle_entity = Entity::filter_by_id(&circle.entity_id).ok_or("Entity not found")?;
        let circle_radius = mass_to_radius(circle_entity.mass);
        let x = circle_entity.position.x + circle.direction.x * circle.magnitude * mass_to_max_move_speed(circle_entity.mass);
        let y = circle_entity.position.y + circle.direction.y * circle.magnitude * mass_to_max_move_speed(circle_entity.mass);
        circle_entity.position.x = x.clamp(circle_radius, world_size as f32 - circle_radius);
        circle_entity.position.y = y.clamp(circle_radius, world_size as f32 - circle_radius);

        // Check to see if we're overlapping with food
        for food in Food::iter() {
            let food_entity = Entity::filter_by_id(&food.entity_id).ok_or("Entity not found")?;
            if is_overlapping(&circle_entity, &food_entity) {
                // We're overlapping with food, so eat it
                Entity::delete_by_id(&food.entity_id);
                Food::delete_by_entity_id(&food.entity_id);
                circle_entity.mass += food_entity.mass;
            }
        }

        // Check to see if we're overlapping with another player
        for other_circle in Circle::iter() {
            if other_circle.entity_id == circle.entity_id {
                continue;
            }
            let other_entity = Entity::filter_by_id(&other_circle.entity_id).ok_or("Entity not found")?;
            let mass_ratio = other_entity.mass as f32 / circle_entity.mass as f32;

            if is_overlapping(&circle_entity, &other_entity) && mass_ratio < 0.85 {
                // We're overlapping with another player, so eat them
                Entity::delete_by_id(&other_entity.id);
                Circle::delete_by_entity_id(&other_circle.entity_id);
                circle_entity.mass += other_entity.mass;
            }
        }

        Entity::update_by_id(&circle_entity.id.clone(), circle_entity);
    }

    schedule!(Duration::from_millis(50), move_all_players());
    Ok(())
}

#[spacetimedb(reducer)]
pub fn spawn_food() -> Result<(), String> {
    // Is there too much food already? Are there no players yet?
    if Food::iter().count() > 600
    // || Circle::iter().count() == 0
    {
        schedule!(Duration::from_millis(500), spawn_food());
        return Ok(());
    }

    let mut rng = rand::thread_rng();
    let food_mass = rng.gen_range(FOOD_MASS_MIN..FOOD_MASS_MAX);
    let world_size = Config::filter_by_id(&0).ok_or("Config not found")?.world_size;
    let food_radius = mass_to_radius(food_mass);
    let x = rng.gen_range(food_radius..world_size as f32 - food_radius);
    let y = rng.gen_range(food_radius..world_size as f32 - food_radius);
    let entity = Entity::insert(Entity {
        id: 0,
        position: Vector2 { x, y },
        mass: food_mass
    })?;
    Food::insert(Food { entity_id: entity.id })?;
    log::info!("Spawned food! {}", entity.id);

    spawn_food().unwrap();
    Ok(())
}

#[spacetimedb(reducer)]
pub fn circle_decay() -> Result<(), String> {
    for circle in Circle::iter() {
        let mut circle_entity = Entity::filter_by_id(&circle.entity_id).ok_or("Entity not found")?;
        if circle_entity.mass <= START_PLAYER_MASS {
            continue;
        }
        circle_entity.mass = (circle_entity.mass as f32 * 0.99) as u32;
        let id = circle_entity.id;
        Entity::update_by_id(&id, circle_entity);
    }

    schedule!(Duration::from_millis(5000), circle_decay());
    Ok(())
}
