pub mod math;

use math::DbVector2;
use rand::Rng;
use spacetimedb::{spacetimedb_lib::ScheduleAt, Identity, ReducerContext, Table, TimeDuration, Timestamp};
use std::{collections::HashMap, time::Duration};

// TODO:
// - [x] Remove players when they are eaten on the client + death + respawn screen
// - [x] Player splitting + increased area of view
// - [x] Overlap amount should be more significant in order to eat
// - [ ] Viruses
// - [ ] Ejecting mass
// - [ ] Leaderboard

const START_PLAYER_MASS: i32 = 15;
const START_PLAYER_SPEED: i32 = 10;
const FOOD_MASS_MIN: i32 = 2;
const FOOD_MASS_MAX: i32 = 4;
const TARGET_FOOD_COUNT: usize = 600;
const MINIMUM_SAFE_MASS_RATIO: f32 = 0.85;

const MIN_MASS_TO_SPLIT: i32 = START_PLAYER_MASS * 2;
const MAX_CIRCLES_PER_PLAYER: i32 = 16;
const SPLIT_RECOMBINE_DELAY_SEC: f32 = 5.0;
const SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC: f32 = 2.0;
const ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT: f32 = 0.9;
const SELF_COLLISION_SPEED: f32 = 0.05; //1 == instantly separate circles. less means separation takes time

#[spacetimedb::table(name = config, public)]
pub struct Config {
    #[primary_key]
    pub id: i32,
    pub world_size: i64,
}

#[spacetimedb::table(name = entity, public)]
#[spacetimedb::table(name = logged_out_entity)]
#[derive(Debug, Clone)]
pub struct Entity {
    #[auto_inc]
    #[primary_key]
    pub entity_id: i32,
    pub position: DbVector2,
    pub mass: i32,
}

#[spacetimedb::table(name = circle, public)]
#[spacetimedb::table(name = logged_out_circle)]
#[derive(Debug, Clone)]
pub struct Circle {
    #[primary_key]
    pub entity_id: i32,
    #[index(btree)]
    pub player_id: i32,
    pub direction: DbVector2,
    pub speed: f32,
    pub last_split_time: Timestamp,
}

#[spacetimedb::table(name = player, public)]
#[spacetimedb::table(name = logged_out_player)]
#[derive(Debug, Clone)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    #[unique]
    #[auto_inc]
    player_id: i32,
    name: String,
}

#[spacetimedb::table(name = food, public)]
pub struct Food {
    #[primary_key]
    pub entity_id: i32,
}

#[spacetimedb::table(name = move_all_players_timer, scheduled(move_all_players))]
pub struct MoveAllPlayersTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::table(name = spawn_food_timer, scheduled(spawn_food))]
pub struct SpawnFoodTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::table(name = circle_decay_timer, scheduled(circle_decay))]
pub struct CircleDecayTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::table(name = circle_recombine_timer, scheduled(circle_recombine))]
pub struct CircleRecombineTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    player_id: i32,
}

#[spacetimedb::table(name = consume_entity_timer, scheduled(consume_entity))]
pub struct ConsumeEntityTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    consumed_entity_id: i32,
    consumer_entity_id: i32,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Initializing...");
    ctx.db.config().try_insert(Config {
        id: 0,
        world_size: 1000,
    })?;
    ctx.db.circle_decay_timer().try_insert(CircleDecayTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_secs(5).into()),
    })?;
    ctx.db.spawn_food_timer().try_insert(SpawnFoodTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(500).into()),
    })?;
    ctx.db.move_all_players_timer().try_insert(MoveAllPlayersTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).into()),
    })?;
    Ok(())
}

#[spacetimedb::reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    if let Some(player) = ctx.db.logged_out_player().identity().find(&ctx.sender) {
        ctx.db.player().insert(player.clone());
        ctx.db.logged_out_player().identity().delete(&player.identity);

        // Restore any circles for this player
        for circle in ctx.db.logged_out_circle().player_id().filter(&player.player_id) {
            ctx.db.logged_out_circle().entity_id().delete(circle.entity_id);
            ctx.db.circle().insert(circle.clone());
            let logged_out_entity = ctx.db.logged_out_entity().entity_id().find(&circle.entity_id).unwrap();
            ctx.db.logged_out_entity().entity_id().delete(circle.entity_id);
            ctx.db.entity().insert(logged_out_entity);
        }
    } else {
        ctx.db.player().try_insert(Player {
            identity: ctx.sender,
            player_id: 0,
            name: String::new(),
        })?;
    }
    Ok(())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnect(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx.db.player().identity().find(&ctx.sender).ok_or("Player not found")?;
    let player_id = player.player_id;
    ctx.db.logged_out_player().insert(player);
    ctx.db.player().identity().delete(&ctx.sender);

    // Move any circles from the arena into logged out tables
    for circle in ctx.db.circle().player_id().filter(&player_id) {
        let entity = ctx.db.entity().entity_id().find(&circle.entity_id).unwrap();
        ctx.db.logged_out_entity().insert(entity);
        ctx.db.entity().entity_id().delete(&circle.entity_id);
        ctx.db.logged_out_circle().insert(circle.clone());
        ctx.db.circle().entity_id().delete(&circle.entity_id);
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn enter_game(ctx: &ReducerContext, name: String) -> Result<(), String> {
    log::info!("Creating player with name {}", name);
    let mut player: Player = ctx.db.player().identity().find(ctx.sender).ok_or("")?;
    let player_id = player.player_id;
    player.name = name;
    ctx.db.player().identity().update(player);
    spawn_player_initial_circle(ctx, player_id)?;

    Ok(())
}

fn spawn_player_initial_circle(ctx: &ReducerContext, player_id: i32) -> Result<Entity, String> {
    let mut rng = ctx.rng();
    let world_size = ctx.db.config().id().find(&0).ok_or("Config not found")?.world_size;
    let player_start_radius = mass_to_radius(START_PLAYER_MASS);
    let x = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    let y = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    spawn_circle_at(ctx, player_id, START_PLAYER_MASS, DbVector2 { x, y }, ctx.timestamp)
}

fn spawn_circle_at(
    ctx: &ReducerContext,
    player_id: i32,
    mass: i32,
    position: DbVector2,
    timestamp: Timestamp,
) -> Result<Entity, String> {
    let entity = ctx.db.entity().try_insert(Entity {
        entity_id: 0,
        position,
        mass,
    })?;

    ctx.db.circle().try_insert(Circle {
        entity_id: entity.entity_id,
        player_id,
        direction: DbVector2 { x: 0.0, y: 1.0 },
        speed: 0.0,
        last_split_time: timestamp,
    })?;
    Ok(entity)
}

#[spacetimedb::reducer]
pub fn respawn(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx
        .db
        .player()
        .identity()
        .find(&ctx.sender)
        .ok_or("No such player found")?;

    spawn_player_initial_circle(ctx, player.player_id)?;

    Ok(())
}

#[spacetimedb::reducer]
pub fn suicide(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx
        .db
        .player()
        .identity()
        .find(&ctx.sender)
        .ok_or("No such player found")?;

    for circle in ctx.db.circle().player_id().filter(&player.player_id) {
        destroy_entity(ctx, circle.entity_id)?;
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn update_player_input(ctx: &ReducerContext, direction: DbVector2) -> Result<(), String> {
    let player = ctx.db.player().identity().find(&ctx.sender).ok_or("Player not found")?;
    for mut circle in ctx.db.circle().player_id().filter(&player.player_id) {
        circle.direction = direction.normalized();
        circle.speed = direction.magnitude().clamp(0.0, 1.0);
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

    // If the distance between the two circle centers is less than the
    // maximum radius, then the center of the smaller circle is inside
    // the larger circle. This gives some leeway for the circles to overlap
    // before being eaten.
    let max_radius = f32::max(radius_a, radius_b);
    distance_sq <= max_radius * max_radius
}

fn mass_to_radius(mass: i32) -> f32 {
    (mass as f32).sqrt()
}

fn mass_to_max_move_speed(mass: i32) -> f32 {
    2.0 * START_PLAYER_SPEED as f32 / (1.0 + (mass as f32 / START_PLAYER_MASS as f32).sqrt())
}

#[spacetimedb::reducer]
pub fn move_all_players(ctx: &ReducerContext, _timer: MoveAllPlayersTimer) -> Result<(), String> {
    // TODO identity check
    // let span = spacetimedb::log_stopwatch::LogStopwatch::new("tick");
    let world_size = ctx.db.config().id().find(0).ok_or("Config not found")?.world_size;

    let mut circle_directions: HashMap<i32, DbVector2> = ctx
        .db
        .circle()
        .iter()
        .map(|c| (c.entity_id, c.direction * c.speed))
        .collect();

    // Split circle movement
    for player in ctx.db.player().iter() {
        let circles: Vec<Circle> = ctx.db.circle().player_id().filter(&player.player_id).collect();
        let mut player_entities: Vec<Entity> = circles
            .iter()
            .map(|c| ctx.db.entity().entity_id().find(&c.entity_id).unwrap())
            .collect();
        if player_entities.len() <= 1 {
            continue;
        }
        let count = player_entities.len();

        // Gravitate circles towards other circles before they recombine
        for i in 0..player_entities.len() {
            let circle_i = &circles[i];
            let time_since_split = ctx
                .timestamp
                .duration_since(circle_i.last_split_time)
                .unwrap()
                .as_secs_f32();
            let time_before_recombining = (SPLIT_RECOMBINE_DELAY_SEC - time_since_split).max(0.0);
            if time_before_recombining > SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC {
                continue;
            }

            let (slice1, slice_i) = player_entities.split_at_mut(i);
            let (slice_i, slice2) = slice_i.split_at_mut(1);
            let entity_i = &mut slice_i[0];
            for entity_j in slice1.iter().chain(slice2.iter()) {
                let mut diff = entity_i.position - entity_j.position;
                let mut distance_sqr = diff.sqr_magnitude();
                if distance_sqr <= 0.0001 {
                    diff = DbVector2::new(1.0, 0.0);
                    distance_sqr = 1.0;
                }
                let radius_sum = mass_to_radius(entity_i.mass) + mass_to_radius(entity_j.mass);
                if distance_sqr > radius_sum * radius_sum {
                    let gravity_multiplier = 1.0 - time_before_recombining / SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC;
                    let vec = diff.normalized() * (radius_sum - distance_sqr.sqrt()) * gravity_multiplier * 0.05
                        / count as f32;
                    *circle_directions.get_mut(&entity_i.entity_id).unwrap() += vec / 2.0;
                    *circle_directions.get_mut(&entity_j.entity_id).unwrap() -= vec / 2.0;
                }
            }
        }

        // Force circles apart
        for i in 0..player_entities.len() {
            let (slice1, slice2) = player_entities.split_at_mut(i + 1);
            let entity_i = &mut slice1[i];
            for j in 0..slice2.len() {
                let entity_j = &mut slice2[j];
                let mut diff = entity_i.position - entity_j.position;
                let mut distance_sqr = diff.sqr_magnitude();
                if distance_sqr <= 0.0001 {
                    diff = DbVector2::new(1.0, 0.0);
                    distance_sqr = 1.0;
                }
                let radius_sum = mass_to_radius(entity_i.mass) + mass_to_radius(entity_j.mass);
                let radius_sum_multiplied = radius_sum * ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT;
                if distance_sqr < radius_sum_multiplied * radius_sum_multiplied {
                    let vec = diff.normalized() * (radius_sum - distance_sqr.sqrt()) * SELF_COLLISION_SPEED;
                    *circle_directions.get_mut(&entity_i.entity_id).unwrap() += vec / 2.0;
                    *circle_directions.get_mut(&entity_j.entity_id).unwrap() -= vec / 2.0;
                }
            }
        }
    }

    // Handle player input
    for circle in ctx.db.circle().iter() {
        let circle_entity = ctx.db.entity().entity_id().find(&circle.entity_id);
        if !circle_entity.is_some() {
            // This can happen if a circle is eaten by another circle
            continue;
        }
        let mut circle_entity = circle_entity.unwrap();
        let circle_radius = mass_to_radius(circle_entity.mass);
        let direction = *circle_directions.get(&circle.entity_id).unwrap();
        let new_pos = circle_entity.position + direction * mass_to_max_move_speed(circle_entity.mass);
        let min = circle_radius;
        let max = world_size as f32 - circle_radius;
        circle_entity.position.x = new_pos.x.clamp(min, max);
        circle_entity.position.y = new_pos.y.clamp(min, max);
        ctx.db.entity().entity_id().update(circle_entity);
    }

    // Check collisions
    let entities: HashMap<i32, Entity> = ctx.db.entity().iter().map(|e| (e.entity_id, e)).collect();
    for circle in ctx.db.circle().iter() {
        // let span = spacetimedb::time_span::Span::start("collisions");
        let circle_entity = entities.get(&circle.entity_id).unwrap();
        for (_, other_entity) in entities.iter() {
            if other_entity.entity_id == circle_entity.entity_id {
                continue;
            }

            if is_overlapping(&circle_entity, other_entity) {
                let other_circle = ctx.db.circle().entity_id().find(&other_entity.entity_id);
                if let Some(other_circle) = other_circle {
                    if other_circle.player_id != circle.player_id {
                        let mass_ratio = other_entity.mass as f32 / circle_entity.mass as f32;
                        if mass_ratio < MINIMUM_SAFE_MASS_RATIO {
                            schedule_consume_entity(ctx, circle_entity.entity_id, other_entity.entity_id);
                        }
                    }
                } else {
                    schedule_consume_entity(ctx, circle_entity.entity_id, other_entity.entity_id);
                }
            }
        }
        // span.end();
    }

    // span.end();
    Ok(())
}

fn schedule_consume_entity(ctx: &ReducerContext, consumer_id: i32, consumed_id: i32) {
    ctx.db.consume_entity_timer().insert(ConsumeEntityTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp.clone()),
        consumer_entity_id: consumer_id,
        consumed_entity_id: consumed_id,
    });
}

#[spacetimedb::reducer]
pub fn consume_entity(ctx: &ReducerContext, request: ConsumeEntityTimer) -> Result<(), String> {
    let consumed_entity = ctx.db.entity().entity_id().find(&request.consumed_entity_id);
    let consumer_entity = ctx.db.entity().entity_id().find(&request.consumer_entity_id);
    if consumed_entity.is_none() {
        return Err("Consumed entity doesn't exist".into());
    }
    if consumer_entity.is_none() {
        return Err("Consumer entity doesn't exist".into());
    }
    let consumed_entity = consumed_entity.unwrap();
    let mut consumer_entity = consumer_entity.unwrap();

    consumer_entity.mass += consumed_entity.mass;
    destroy_entity(ctx, consumed_entity.entity_id)?;
    ctx.db.entity().entity_id().update(consumer_entity);

    Ok(())
}

pub fn destroy_entity(ctx: &ReducerContext, entity_id: i32) -> Result<(), String> {
    ctx.db.food().entity_id().delete(&entity_id);
    ctx.db.circle().entity_id().delete(&entity_id);
    ctx.db.entity().entity_id().delete(&entity_id);

    Ok(())
}

#[spacetimedb::reducer]
pub fn player_split(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx
        .db
        .player()
        .identity()
        .find(&ctx.sender)
        .ok_or("Sender has no player")?;
    let circles: Vec<Circle> = ctx.db.circle().player_id().filter(&player.player_id).collect();
    let mut circle_count = circles.len() as i32;
    if circle_count >= MAX_CIRCLES_PER_PLAYER {
        return Ok(());
    }

    for mut circle in circles {
        let mut circle_entity = ctx
            .db
            .entity()
            .entity_id()
            .find(&circle.entity_id)
            .ok_or("Circle has no entity")?;
        if circle_entity.mass >= MIN_MASS_TO_SPLIT * 2 {
            let half_mass = circle_entity.mass / 2;
            spawn_circle_at(
                ctx,
                circle.player_id,
                half_mass,
                circle_entity.position + circle.direction,
                ctx.timestamp,
            )?;
            circle_entity.mass -= half_mass;
            circle.last_split_time = ctx.timestamp;
            ctx.db.circle().entity_id().update(circle);
            ctx.db.entity().entity_id().update(circle_entity);
            circle_count += 1;
            if circle_count >= MAX_CIRCLES_PER_PLAYER {
                break;
            }
        }
    }

    ctx.db.circle_recombine_timer().insert(CircleRecombineTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(
            ctx.timestamp + TimeDuration::from(Duration::from_secs_f32(SPLIT_RECOMBINE_DELAY_SEC)),
        ),
        player_id: player.player_id,
    });

    log::warn!("Player split!");

    Ok(())
}

#[spacetimedb::reducer]
pub fn spawn_food(ctx: &ReducerContext, _timer: SpawnFoodTimer) -> Result<(), String> {
    if ctx.db.player().count() == 0 {
        //Are there no players yet?
        return Ok(());
    }

    let world_size = ctx.db.config().id().find(0).ok_or("Config not found")?.world_size;

    let mut rng = ctx.rng();
    let mut food_count = ctx.db.food().count();
    while food_count < TARGET_FOOD_COUNT as u64 {
        let food_mass = rng.gen_range(FOOD_MASS_MIN..FOOD_MASS_MAX);
        let food_radius = mass_to_radius(food_mass);
        let x = rng.gen_range(food_radius..world_size as f32 - food_radius);
        let y = rng.gen_range(food_radius..world_size as f32 - food_radius);
        let entity = ctx.db.entity().try_insert(Entity {
            entity_id: 0,
            position: DbVector2 { x, y },
            mass: food_mass,
        })?;
        ctx.db.food().try_insert(Food {
            entity_id: entity.entity_id,
        })?;
        food_count += 1;
        log::info!("Spawned food! {}", entity.entity_id);
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn circle_decay(ctx: &ReducerContext, _timer: CircleDecayTimer) -> Result<(), String> {
    for circle in ctx.db.circle().iter() {
        let mut circle_entity = ctx
            .db
            .entity()
            .entity_id()
            .find(&circle.entity_id)
            .ok_or("Entity not found")?;
        if circle_entity.mass <= START_PLAYER_MASS {
            continue;
        }
        circle_entity.mass = (circle_entity.mass as f32 * 0.99) as i32;
        ctx.db.entity().entity_id().update(circle_entity);
    }

    Ok(())
}

pub fn calculate_center_of_mass(entities: &[Entity]) -> DbVector2 {
    let total_mass: i32 = entities.iter().map(|e| e.mass).sum();
    let center_of_mass: DbVector2 = entities.iter().map(|e| e.position * e.mass as f32).sum();
    center_of_mass / total_mass as f32
}

#[spacetimedb::reducer]
pub fn circle_recombine(ctx: &ReducerContext, timer: CircleRecombineTimer) -> Result<(), String> {
    let circles: Vec<Circle> = ctx.db.circle().player_id().filter(&timer.player_id).collect();
    let recombining_entities: Vec<Entity> = circles
        .iter()
        .filter(|c| ctx.timestamp.duration_since(c.last_split_time).unwrap().as_secs_f32() >= SPLIT_RECOMBINE_DELAY_SEC)
        .map(|c| ctx.db.entity().entity_id().find(&c.entity_id).unwrap())
        .collect();
    if recombining_entities.len() <= 1 {
        return Ok(()); //No circles to recombine
    }

    let base_entity_id = recombining_entities[0].entity_id;
    for i in 1..recombining_entities.len() {
        schedule_consume_entity(ctx, base_entity_id, recombining_entities[i].entity_id);
    }

    Ok(())
}
