//! STDB module used for benchmarks based on "realistic" workloads we are focusing in improving.

#![allow(clippy::too_many_arguments, unused_variables)]

use crate::Load;
use spacetimedb::{log, ReducerContext, SpacetimeType, Table};
use std::hash::{Hash, Hasher};

#[spacetimedb::table(name = velocity)]
pub struct Velocity {
    #[primary_key]
    pub entity_id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Velocity {
    pub fn new(entity_id: u32, x: f32, y: f32, z: f32) -> Self {
        Self { entity_id, x, y, z }
    }
}

#[spacetimedb::table(name = position)]
pub struct Position {
    #[primary_key]
    pub entity_id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
}

impl Position {
    pub fn new(entity_id: u32, x: f32, y: f32, z: f32) -> Self {
        Self {
            entity_id,
            x,
            y,
            z,
            vx: x + 10.0,
            vy: y + 20.0,
            vz: z + 30.0,
        }
    }
}

pub fn moment_milliseconds() -> u64 {
    1
    // Duration::from_micros(1000).as_millis() as u64
    // or previously...
    // Timestamp::from_micros_since_epoch(1000)
    //     .duration_since(Timestamp::UNIX_EPOCH)
    //     .ok()
    //     .unwrap()
    //     .as_millis() as u64
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub enum AgentAction {
    Inactive,
    Idle,
    Evading,
    Investigating,
    Retreating,
    Fighting,
}

#[spacetimedb::table(name = game_enemy_ai_agent_state)]
#[derive(Clone)]
pub struct GameEnemyAiAgentState {
    #[primary_key]
    pub entity_id: u64,
    pub last_move_timestamps: Vec<u64>,
    pub next_action_timestamp: u64,
    pub action: AgentAction,
}

#[spacetimedb::table(name = game_targetable_state)]
#[derive(Clone)]
pub struct GameTargetableState {
    #[primary_key]
    pub entity_id: u64,
    pub quad: i64,
}

#[spacetimedb::table(name = game_live_targetable_state)]
pub struct GameLiveTargetableState {
    #[unique]
    pub entity_id: u64,
    #[index(btree)]
    pub quad: i64,
}

#[spacetimedb::table(name = game_mobile_entity_state)]
pub struct GameMobileEntityState {
    #[primary_key]
    pub entity_id: u64,

    #[index(btree)]
    pub location_x: i32,
    pub location_y: i32,
    pub timestamp: u64,
}

#[spacetimedb::table(name = game_enemy_state)]
#[derive(Clone)]
pub struct GameEnemyState {
    #[primary_key]
    pub entity_id: u64,
    pub herd_id: i32,
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq)]
pub struct SmallHexTile {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[spacetimedb::table(name = game_herd_cache)]
#[derive(Clone, Debug)]
pub struct GameHerdCache {
    #[primary_key]
    pub id: i32,
    pub dimension_id: u32,
    pub current_population: i32,
    pub location: SmallHexTile,
    pub max_population: i32,
    pub spawn_eagerness: f32,
    pub roaming_distance: i32,
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

// ---------- insert bulk ----------
#[spacetimedb::reducer]
pub fn insert_bulk_position(ctx: &ReducerContext, count: u32) {
    for id in 0..count {
        ctx.db
            .position()
            .insert(Position::new(id, id as f32, (id + 5) as f32, (id * 5) as f32));
    }
    log::info!("INSERT POSITION: {count}");
}

#[spacetimedb::reducer]
pub fn insert_bulk_velocity(ctx: &ReducerContext, count: u32) {
    for id in 0..count {
        ctx.db
            .velocity()
            .insert(Velocity::new(id, id as f32, (id + 5) as f32, (id * 5) as f32));
    }
    log::info!("INSERT VELOCITY: {count}");
}

// Simulate
// ```
// UPDATE Position SET
// x = x + vx,
// y = y + vy,
// z = z + vz;
// ```
#[spacetimedb::reducer]
pub fn update_position_all(ctx: &ReducerContext, expected: u32) {
    let mut count = 0;
    for mut position in ctx.db.position().iter() {
        position.x += position.vx;
        position.y += position.vy;
        position.z += position.vz;

        let id = position.entity_id;
        ctx.db.position().entity_id().update(position);
        count += 1;
    }
    log::info!("UPDATE POSITION ALL: {expected}, processed: {count}");
}

// Simulate
// ```
// UPDATE Position
// SET
//     x = Position.x + Velocity.x,
//     y = Position.y + Velocity.y,
//     z = Position.z + Velocity.z
// FROM Velocity
// WHERE Position.entity_id = Velocity.entity_id;
// ```
#[spacetimedb::reducer]
pub fn update_position_with_velocity(ctx: &ReducerContext, expected: u32) {
    let mut count = 0;
    for velocity in ctx.db.velocity().iter() {
        let Some(mut position) = ctx.db.position().entity_id().find(velocity.entity_id) else {
            continue;
        };

        position.x += velocity.x;
        position.y += velocity.y;
        position.z += velocity.z;

        let id = position.entity_id;
        ctx.db.position().entity_id().update(position);
        count += 1;
    }
    log::info!("UPDATE POSITION BY VELOCITY: {expected}, processed: {count}");
}

// Simulations for a game loop

#[spacetimedb::reducer]
pub fn insert_world(ctx: &ReducerContext, players: u64) {
    for (i, id) in (0..players).enumerate() {
        let next_action_timestamp = if i & 2 == 2 {
            moment_milliseconds() + 2000 // Check every 2secs
        } else {
            moment_milliseconds()
        };

        ctx.db.game_enemy_ai_agent_state().insert(GameEnemyAiAgentState {
            entity_id: id,
            next_action_timestamp,
            last_move_timestamps: vec![id, 0, id * 2],
            action: AgentAction::Idle,
        });

        ctx.db.game_live_targetable_state().insert(GameLiveTargetableState {
            entity_id: id,
            quad: id as i64,
        });

        ctx.db.game_targetable_state().insert(GameTargetableState {
            entity_id: id,
            quad: id as i64,
        });

        ctx.db.game_mobile_entity_state().insert(GameMobileEntityState {
            entity_id: id,
            location_x: id as i32,
            location_y: id as i32,
            timestamp: next_action_timestamp,
        });

        ctx.db.game_enemy_state().insert(GameEnemyState {
            entity_id: id,
            herd_id: id as i32,
        });

        ctx.db.game_herd_cache().insert(GameHerdCache {
            id: id as i32,
            dimension_id: id as u32,
            current_population: id as i32 * 2,
            max_population: id as i32 * 4,
            spawn_eagerness: id as f32,
            roaming_distance: id as i32,
            location: SmallHexTile {
                x: id as i32,
                z: id as i32,
                dimension: id as u32 * 2,
            },
        });
    }
    log::info!("INSERT WORLD PLAYERS: {players}");
}

fn get_targetables_near_quad(ctx: &ReducerContext, entity_id: u64, num_players: u64) -> Vec<GameTargetableState> {
    let mut result = Vec::with_capacity(4);

    for id in entity_id..num_players {
        for t in ctx.db.game_live_targetable_state().quad().filter(&(id as i64)) {
            result.push(
                ctx.db
                    .game_targetable_state()
                    .entity_id()
                    .find(t.entity_id)
                    .expect("Identity not found"),
            )
        }
    }

    result
}

const MAX_MOVE_TIMESTAMPS: usize = 20;
fn move_agent(
    ctx: &ReducerContext,
    agent: &mut GameEnemyAiAgentState,
    agent_coord: SmallHexTile,
    current_time_ms: u64,
) {
    let entity_id = agent.entity_id;

    let enemy = ctx
        .db
        .game_enemy_state()
        .entity_id()
        .find(entity_id)
        .expect("GameEnemyState Entity ID not found")
        .clone();
    ctx.db.game_enemy_state().entity_id().update(enemy);

    agent.next_action_timestamp = current_time_ms + 2000;

    // Keep track of the last [MAX_MOVE_TIMESTAMPS] movements
    agent.last_move_timestamps.push(current_time_ms);
    if agent.last_move_timestamps.len() > MAX_MOVE_TIMESTAMPS {
        agent.last_move_timestamps.remove(0);
    }

    // Update targetable to the destination
    let mut targetable = ctx
        .db
        .game_targetable_state()
        .entity_id()
        .find(entity_id)
        .expect("GameTargetableState Entity ID not found");
    let new_hash = calculate_hash(&targetable.quad) as i64;
    targetable.quad = new_hash;
    ctx.db.game_targetable_state().entity_id().update(targetable);

    // If the entity is alive (which it should be),
    // also update the `LiveTargetableState` used by `enemy_ai_agent_loop`.
    if ctx
        .db
        .game_live_targetable_state()
        .entity_id()
        .find(entity_id)
        .is_some()
    {
        ctx.db
            .game_live_targetable_state()
            .entity_id()
            .update(GameLiveTargetableState {
                entity_id,
                quad: new_hash,
            });
    }
    let mobile_entity = ctx
        .db
        .game_mobile_entity_state()
        .entity_id()
        .find(entity_id)
        .expect("GameMobileEntityState Entity ID not found");
    let mobile_entity = GameMobileEntityState {
        entity_id,
        location_x: mobile_entity.location_x + 1,
        location_y: mobile_entity.location_y + 1,
        timestamp: agent.next_action_timestamp,
    };

    ctx.db.game_enemy_ai_agent_state().entity_id().update(agent.clone());

    ctx.db.game_mobile_entity_state().entity_id().update(mobile_entity);
}

fn agent_loop(
    ctx: &ReducerContext,
    mut agent: GameEnemyAiAgentState,
    agent_targetable: GameTargetableState,
    surrounding_agents: &[GameTargetableState],
    current_time_ms: u64,
) {
    let entity_id = agent.entity_id;
    let coordinates = ctx
        .db
        .game_mobile_entity_state()
        .entity_id()
        .find(entity_id)
        .expect("GameMobileEntityState Entity ID not found");

    let agent_entity = ctx
        .db
        .game_enemy_state()
        .entity_id()
        .find(entity_id)
        .expect("GameEnemyState Entity ID not found");
    let agent_herd = ctx
        .db
        .game_herd_cache()
        .id()
        .find(agent_entity.herd_id)
        .expect("GameHerdCache Entity ID not found");
    let agent_herd_coordinates = agent_herd.location;

    move_agent(ctx, &mut agent, agent_herd_coordinates, current_time_ms);
}

// We check only for a single pass in the game loop.
#[spacetimedb::reducer]
pub fn game_loop_enemy_ia(ctx: &ReducerContext, players: u64) {
    let mut count = 0;
    let current_time_ms = moment_milliseconds();

    for mut agent in ctx.db.game_enemy_ai_agent_state().iter() {
        let agent_targetable = ctx
            .db
            .game_targetable_state()
            .entity_id()
            .find(agent.entity_id)
            .expect("No TargetableState for AgentState entity");

        let surrounding_agents = get_targetables_near_quad(ctx, agent_targetable.entity_id, players);

        agent.action = AgentAction::Fighting;

        agent_loop(ctx, agent, agent_targetable, &surrounding_agents, current_time_ms);

        count += 1;
    }

    log::info!("ENEMY IA LOOP PLAYERS: {players}, processed: {count}");
}

#[spacetimedb::reducer]
pub fn init_game_ia_loop(ctx: &ReducerContext, initial_load: u32) {
    let load = Load::new(initial_load);

    insert_bulk_position(ctx, load.biggest_table);
    insert_bulk_velocity(ctx, load.big_table);
    update_position_all(ctx, load.biggest_table);
    update_position_with_velocity(ctx, load.big_table);

    insert_world(ctx, load.num_players as u64);
}

#[spacetimedb::reducer]
pub fn run_game_ia_loop(ctx: &ReducerContext, initial_load: u32) {
    let load = Load::new(initial_load);

    game_loop_enemy_ia(ctx, load.num_players as u64);
}
