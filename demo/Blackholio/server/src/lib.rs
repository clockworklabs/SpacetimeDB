use rand::Rng;
use spacetimedb::{spacetimedb_lib::ScheduleAt, ReducerContext, SpacetimeType, Table};
use spacetimedb::log;
use std::time::Duration;

// TODO:
// - [x] Remove players when they are eaten on the client + death + respawn screen
// - [ ] Player splitting + increased area of view
// - [x] Overlap amount should be more significant in order to eat
// - [ ] Viruses
// - [ ] Ejecting mass
// - [ ] Leaderboard

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
    pub velocity: Vector2,
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

#[spacetimedb::table(name = move_all_players_timer, scheduled(move_all_players))]
pub struct MoveAllPlayersTimer {}

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

const START_PLAYER_MASS_MIN: u32 = 20;
const START_PLAYER_MASS_MAX: u32 = 1000;
const START_PLAYER_SPEED: u32 = 20;
const CIRCLE_COUNT: u32 = 300;

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Initializing...");
    ctx.db.config().try_insert(Config { id: 0, world_size: 2000 })?;
    ctx.db.move_all_players_timer().try_insert(MoveAllPlayersTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).as_micros() as u64),
    })?;
    for _ in 0..CIRCLE_COUNT {
        spawn_circle(ctx)?;
    }
    Ok(())
}

fn spawn_circle(ctx: &ReducerContext) -> Result<Entity, String> {
    let mut rng = ctx.rng();
    let world_size = ctx.db.config().id().find(&0).ok_or("Config not found")?.world_size;
    let mass = rng.gen_range(START_PLAYER_MASS_MIN..START_PLAYER_MASS_MAX);
    let player_start_radius = mass_to_radius(mass);
    let x = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    let y = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    spawn_circle_at(ctx, mass, x, y)
}

fn spawn_circle_at(ctx: &ReducerContext, mass: u32, x: f32, y: f32) -> Result<Entity, String> {
    let entity = ctx.db.entity().try_insert(Entity {
        id: 0,
        position: Vector2 { x, y },
        mass,
    })?;

    let mut rng = ctx.rng();

    ctx.db.circle().try_insert(Circle {
        entity_id: entity.id,
        velocity: Vector2 {
            x: rng.gen_range(-(START_PLAYER_SPEED as i32)..(START_PLAYER_SPEED as i32)) as f32,
            y: rng.gen_range(-(START_PLAYER_SPEED as i32)..(START_PLAYER_SPEED as i32)) as f32,
        },
    })?;
    Ok(entity)
}

fn mass_to_radius(mass: u32) -> f32 {
    (mass as f32).sqrt()
}

fn elastic(v1: Vector2, v2: Vector2, m1: f32, m2: f32) -> (Vector2, Vector2) {
    let total_mass = m1 + m2;
    let v1_new = Vector2 {
        x: (v1.x * (m1 - m2) + 2.0 * m2 * v2.x) / total_mass,
        y: (v1.y * (m1 - m2) + 2.0 * m2 * v2.y) / total_mass,
    };
    let v2_new = Vector2 {
        x: (v2.x * (m2 - m1) + 2.0 * m1 * v1.x) / total_mass,
        y: (v2.y * (m2 - m1) + 2.0 * m1 * v1.y) / total_mass,
    };
//    log::info!("Elastic collision between: {:?} and {:?}", v1, v2);
//    log::info!("Elastic collision mass: {:?} and {:?}", m1, m2);
//    log::info!("Elastic collision result: {:?} and {:?}", v1_new, v2_new);
    (v1_new, v2_new)
}

#[spacetimedb::reducer]
pub fn move_all_players(ctx: &ReducerContext, _timer: MoveAllPlayersTimer) -> Result<(), String> {
    let span = spacetimedb::log_stopwatch::LogStopwatch::new("tick");
    let world_size = ctx.db.config().id().find(0).ok_or("Config not found")?.world_size;
    for mut circle in ctx.db.circle().iter() {
        let Some(mut circle_entity) = ctx.db.entity().id().find(&circle.entity_id) else {
            continue;
        };
        let circle_radius = mass_to_radius(circle_entity.mass);
        let x = circle_entity.position.x + circle.velocity.x;
        let y = circle_entity.position.y + circle.velocity.y;
        circle_entity.position.x = x.clamp(circle_radius, world_size as f32 - circle_radius);
        circle_entity.position.y = y.clamp(circle_radius, world_size as f32 - circle_radius);

        if circle_entity.position.x <= circle_radius {
            circle.velocity.x = -circle.velocity.x;
        } else if circle_entity.position.x >= world_size as f32 - circle_radius {
            circle.velocity.x = -circle.velocity.x;
        }

        if circle_entity.position.y <= circle_radius {
            circle.velocity.y = -circle.velocity.y;
        } else if circle_entity.position.y >= world_size as f32 - circle_radius {
            circle.velocity.y = -circle.velocity.y;
        }

        ctx.db.circle().entity_id().update(circle);
        ctx.db.entity().id().update(circle_entity);
    }

    {
        let span = spacetimedb::log_stopwatch::LogStopwatch::new("collisions");
        for (idx, mut circle) in ctx.db.circle().iter().enumerate() {
            let Some(mut circle_entity) = ctx.db.entity().id().find(&circle.entity_id) else { continue; };
            // Check collisions
            for mut other_circle in ctx.db.circle().iter().skip(idx+1) {
                let Some(mut other_entity) = ctx.db.entity().id().find(&other_circle.entity_id) else { continue; };
                let overlap_vector = Vector2 {
                    x: circle_entity.position.x - other_entity.position.x,
                    y: circle_entity.position.y - other_entity.position.y,
                };
                let overlap_length = mass_to_radius(circle_entity.mass) + mass_to_radius(other_entity.mass) - (overlap_vector.x*overlap_vector.x + overlap_vector.y*overlap_vector.y).sqrt();
                if overlap_length <= 0.0 {
                    continue;
                }
                let overlap_vector = overlap_vector.normalize();

                circle_entity.position.x += overlap_vector.x * overlap_length / 2.0;
                circle_entity.position.y += overlap_vector.y * overlap_length / 2.0;
                other_entity.position.x -= overlap_vector.x * overlap_length / 2.0;
                other_entity.position.y -= overlap_vector.y * overlap_length / 2.0;

                let (circle_velocity, other_circle_velocity) = elastic(circle.velocity, other_circle.velocity, circle_entity.mass as f32, other_entity.mass as f32);
                circle.velocity = circle_velocity;
                other_circle.velocity = other_circle_velocity;

                ctx.db.circle().entity_id().update(other_circle);
                ctx.db.entity().id().update(other_entity);

            }
            ctx.db.circle().entity_id().update(circle);
            ctx.db.entity().id().update(circle_entity);
        }
        span.end();
    }

    span.end();
    Ok(())
}
