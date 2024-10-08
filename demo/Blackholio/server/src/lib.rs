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

#[spacetimedb::table(name = spawn_food_timer, scheduled(spawn_food))]
pub struct SpawnFoodTimer {}

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

fn mass_to_radius(mass: u32) -> f32 {
    (mass as f32).sqrt()
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
