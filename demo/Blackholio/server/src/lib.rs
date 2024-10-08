use rand::Rng;
use spacetimedb::{spacetimedb_lib::ScheduleAt, Identity, ReducerContext, SpacetimeType, Table, Timestamp};
use std::time::Duration;

// TODO:
// - [x] Remove players when they are eaten on the client + death + respawn screen
// - [ ] Player splitting + increased area of view
// - [x] Overlap amount should be more significant in order to eat
// - [ ] Viruses
// - [ ] Ejecting mass
// - [ ] Leaderboard

const TARGET_FOOD_COUNT: usize = 600;
const MINIMUM_SAFE_MASS_RATIO: f32 = 0.85;

#[spacetimedb::table(name = config, public)]
pub struct Config {
    #[primary_key]
    pub id: u32,
    pub world_size: u64,
}

#[spacetimedb::table(name = entity, public)]
pub struct Entity {
    #[auto_inc]
    #[primary_key]
    pub id: u32,
    pub position: Vector2,
    pub mass: u32,
}

#[spacetimedb::table(name = circle, public)]
pub struct Circle {
    #[primary_key]
    pub entity_id: u32,
    #[index(btree)]
    pub player_id: u32,
    pub direction: Vector2,
    pub magnitude: f32,
    pub last_split_time: Timestamp,
}

#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    #[unique]
    #[auto_inc]
    player_id: u32,
    name: String,
}

#[spacetimedb::table(name = logged_out_player, public)]
pub struct LoggedOutPlayer {
    #[primary_key]
    identity: Identity,
    player: Player,
}

#[spacetimedb::table(name = logged_out_circle, public)]
pub struct LoggedOutCircle {
    #[auto_inc]
    #[primary_key]
    logged_out_id: u32,
    #[index(btree)]
    player_id: u32,
    circle: Circle,
    entity: Entity,
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

#[spacetimedb::table(name = move_all_players_timer, scheduled(move_all_players))]
pub struct MoveAllPlayersTimer {}

#[spacetimedb::table(name = spawn_food_timer, scheduled(spawn_food))]
pub struct SpawnFoodTimer {}

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

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Initializing...");
    ctx.db.config().try_insert(Config { id: 0, world_size: 1000 })?;
    ctx.db.spawn_food_timer().try_insert(SpawnFoodTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(500).as_micros() as u64),
    })?;
    ctx.db.move_all_players_timer().try_insert(MoveAllPlayersTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).as_micros() as u64),
    })?;
    Ok(())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnect(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx.db.player().identity().find(&ctx.sender).ok_or("Player not found")?;
    for circle in ctx.db.circle().player_id().filter(&player.player_id) {
        let entity = ctx.db.entity().id().find(&circle.entity_id).ok_or("Could not find circle")?;
        ctx.db.entity().id().delete(&entity.id);
        ctx.db.circle().entity_id().delete(&entity.id);
        ctx.db.logged_out_circle().try_insert(LoggedOutCircle {
            logged_out_id: 0,
            player_id: player.player_id,
            circle,
            entity,
        })?;
    }
    ctx.db.logged_out_player().insert(LoggedOutPlayer {
        identity: player.identity,
        player,
    });
    ctx.db.player().identity().delete(&ctx.sender);

    Ok(())
}

#[spacetimedb::reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx.db.logged_out_player().identity().find(&ctx.sender).ok_or("No player for identity.")?;
    for logged_out_circle in ctx.db.logged_out_circle().player_id().filter(&player.player.player_id) {
        ctx.db.circle().try_insert(logged_out_circle.circle)?;
        ctx.db.entity().try_insert(logged_out_circle.entity)?;
        ctx.db.logged_out_circle().logged_out_id().delete(&logged_out_circle.logged_out_id);
    }
    ctx.db.player().insert(player.player);
    Ok(())
}

#[spacetimedb::reducer]
pub fn create_player(ctx: &ReducerContext, name: String) -> Result<(), String> {
    log::info!("Creating player with name {}", name);
    let player = ctx.db.player().try_insert(Player {
        identity: ctx.sender,
        player_id: 0,
        name,
    })?;
    spawn_circle(ctx, player.player_id, ctx.timestamp)?;

    Ok(())
}

#[spacetimedb::reducer]
pub fn respawn(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx.db.player().identity().find(&ctx.sender).ok_or("No such player found")?;
    spawn_circle(ctx, player.player_id, ctx.timestamp)?;
    Ok(())
}

fn spawn_circle(ctx: &ReducerContext, player_id: u32, current_time: Timestamp) -> Result<Entity, String> {
    let mut rng = ctx.rng();
    let world_size = ctx.db.config().id().find(&0).ok_or("Config not found")?.world_size;
    let player_start_radius = mass_to_radius(START_PLAYER_MASS);
    let x = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    let y = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    spawn_circle_at(ctx, player_id, START_PLAYER_MASS, x, y, current_time)
}

fn spawn_circle_at(ctx: &ReducerContext, player_id: u32, mass: u32, x: f32, y: f32, current_time: Timestamp) -> Result<Entity, String> {
    let entity = ctx.db.entity().try_insert(Entity {
        id: 0,
        position: Vector2 { x, y },
        mass,
    })?;

    ctx.db.circle().try_insert(Circle {
        entity_id: entity.id,
        player_id,
        direction: Vector2 { x: 0.0, y: 1.0 },
        magnitude: 0.0,
        last_split_time: current_time
    })?;
    Ok(entity)
}

#[spacetimedb::reducer]
pub fn update_player_input(ctx: &ReducerContext,
                           direction: Vector2, magnitude: f32) -> Result<(), String> {
    let player = ctx.db.player().identity().find(&ctx.sender).ok_or("Player not found")?;
    for mut circle in ctx.db.circle().player_id().filter(&player.player_id) {
        circle.direction = direction.normalize();
        circle.magnitude = magnitude.clamp(0.0, 1.0);
        ctx.db.circle().entity_id().update(circle);
    }
    Ok(())
}

fn is_overlapping(a: &Entity, b: &Entity) -> bool {
    let dx = a.position.x - b.position.x;
    let dy = a.position.y - b.position.y;
    let distance_sq = dx * dx + dy * dy;

    let radius_a = mass_to_radius(a.mass);
    let radius_b = mass_to_radius(b.mass);
    let radius_sum = radius_a + radius_b;

    distance_sq <= radius_sum * radius_sum
}

fn mass_to_radius(mass: u32) -> f32 {
    (mass as f32).sqrt()
}

fn mass_to_max_move_speed(mass: u32) -> f32 {
    2.0 * START_PLAYER_SPEED as f32 / (1.0 + (mass as f32 / START_PLAYER_MASS as f32).sqrt())
}

#[spacetimedb::reducer]
pub fn move_all_players(ctx: &ReducerContext, _timer: MoveAllPlayersTimer) -> Result<(), String> {
    let span = spacetimedb::log_stopwatch::LogStopwatch::new("tick");
    let world_size = ctx.db.config().id().find(0).ok_or("Config not found")?.world_size;
    for circle in ctx.db.circle().iter() {
        let Some(mut circle_entity) = ctx.db.entity().id().find(&circle.entity_id) else {
            continue;
        };
        let circle_radius = mass_to_radius(circle_entity.mass);
        let x = circle_entity.position.x + circle.direction.x * circle.magnitude * mass_to_max_move_speed(circle_entity.mass);
        let y = circle_entity.position.y + circle.direction.y * circle.magnitude * mass_to_max_move_speed(circle_entity.mass);
        circle_entity.position.x = x.clamp(circle_radius, world_size as f32 - circle_radius);
        circle_entity.position.y = y.clamp(circle_radius, world_size as f32 - circle_radius);

        // Check collisions
        // let span = spacetimedb::time_span::Span::start("collisions");
        for entity in ctx.db.entity().iter() {
            if entity.id == circle_entity.id {
                continue;
            }
            if is_overlapping(&circle_entity, &entity) {
                // Check to see if we're overlapping with food
                if ctx.db.food().entity_id().find(&entity.id).is_some() {
                    ctx.db.entity().id().delete(&entity.id);
                    ctx.db.food().entity_id().delete(&entity.id);
                    circle_entity.mass += entity.mass;
                }

                // Check to see if we're overlapping with another circle owned by another player
                let other_circle = ctx.db.circle().entity_id().find(&entity.id);
                if let Some(other_circle) = other_circle {
                    if other_circle.player_id != circle.player_id {
                        let mass_ratio = entity.mass as f32 / circle_entity.mass as f32;
                        if mass_ratio < MINIMUM_SAFE_MASS_RATIO {
                            ctx.db.entity().id().delete(&entity.id);
                            ctx.db.circle().entity_id().delete(&entity.id);
                            circle_entity.mass += entity.mass;
                        }
                    }
                }
            }
        }
        // span.end();

        ctx.db.entity().id().update(circle_entity);
    }

    span.end();
    Ok(())
}

#[spacetimedb::reducer]
pub fn player_split(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx.db.player().identity().find(&ctx.sender).ok_or("Sender has no player")?;
    for mut circle in ctx.db.circle().player_id().filter(&player.player_id) {
        let mut circle_entity = ctx.db.entity().id().find(&circle.entity_id).ok_or("Circle has no entity")?;
        if circle_entity.mass >= START_PLAYER_MASS * 2 {
            let half_mass = circle_entity.mass / 2;
            let extra_mass = circle_entity.mass % 2;
            spawn_circle_at(ctx, circle.player_id, half_mass, circle_entity.position.x,
                                                   circle_entity.position.y, ctx.timestamp)?;
            circle_entity.mass = half_mass + extra_mass;
            circle.last_split_time = ctx.timestamp;
            ctx.db.circle().entity_id().update(circle);
            ctx.db.entity().id().update(circle_entity);
        }
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn spawn_food(ctx: &ReducerContext, _timer: SpawnFoodTimer) -> Result<(), String> {
    // Is there too much food already? Are there no players yet?
    let mut food_count = ctx.db.food().count();
    let player_count = ctx.db.player().count();

    while food_count < TARGET_FOOD_COUNT as u64 && player_count > 0 {
        let mut rng = ctx.rng();
        let food_mass = rng.gen_range(FOOD_MASS_MIN..FOOD_MASS_MAX);
        let world_size = ctx.db.config().id().find(0).ok_or("Config not found")?.world_size;
        let food_radius = mass_to_radius(food_mass);
        let x = rng.gen_range(food_radius..world_size as f32 - food_radius);
        let y = rng.gen_range(food_radius..world_size as f32 - food_radius);
        let entity = ctx.db.entity().try_insert(Entity {
            id: 0,
            position: Vector2 { x, y },
            mass: food_mass
        })?;
        ctx.db.food().try_insert(Food { entity_id: entity.id })?;
        food_count += 1;
        log::info!("Spawned food! {}", entity.id);
    }

    Ok(())
}
