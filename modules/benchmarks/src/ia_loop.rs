//! STDB module used for benchmarks based on "realistic" workloads we are focusing in improving.

#![allow(clippy::too_many_arguments, unused_variables)]

use crate::Load;
use spacetimedb::{log, spacetimedb, SpacetimeType, Timestamp};
use std::hash::{Hash, Hasher};

#[spacetimedb(table)]
pub struct Velocity {
    #[primarykey]
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

#[spacetimedb(table)]
pub struct Position {
    #[primarykey]
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
    Timestamp::from_micros_since_epoch(1000)
        .duration_since(Timestamp::UNIX_EPOCH)
        .ok()
        .unwrap()
        .as_millis() as u64
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

#[spacetimedb(table)]
#[derive(Clone)]
pub struct GameEnemyAiAgentState {
    #[primarykey]
    pub entity_id: u64,
    pub last_move_timestamps: Vec<u64>,
    pub next_action_timestamp: u64,
    pub action: AgentAction,
}

#[spacetimedb(table)]
#[derive(Clone)]
pub struct GameTargetableState {
    #[primarykey]
    pub entity_id: u64,
    pub quad: i64,
}

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "LiveTargetableState_quad", quad))]
pub struct GameLiveTargetableState {
    #[unique]
    pub entity_id: u64,
    pub quad: i64,
}

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "x", location_x))]
pub struct GameMobileEntityState {
    #[primarykey]
    pub entity_id: u64,

    pub location_x: i32,
    pub location_y: i32,
    pub timestamp: u64,
}

#[spacetimedb(table)]
#[derive(Clone)]
pub struct GameEnemyState {
    #[primarykey]
    pub entity_id: u64,
    pub herd_id: i32,
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq)]
pub struct SmallHexTile {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[spacetimedb(table)]
#[derive(Clone, Debug)]
pub struct GameHerdCache {
    #[primarykey]
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
#[spacetimedb(reducer)]
pub fn insert_bulk_position(count: u32) {
    for id in 0..count {
        Position::insert(Position::new(id, id as f32, (id + 5) as f32, (id * 5) as f32)).unwrap();
    }
    log::info!("INSERT POSITION: {count}");
}

#[spacetimedb(reducer)]
pub fn insert_bulk_velocity(count: u32) {
    for id in 0..count {
        Velocity::insert(Velocity::new(id, id as f32, (id + 5) as f32, (id * 5) as f32)).unwrap();
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
#[spacetimedb(reducer)]
pub fn update_position_all(expected: u32) {
    let mut count = 0;
    for mut position in Position::iter() {
        position.x += position.vx;
        position.y += position.vy;
        position.z += position.vz;

        let id = position.entity_id;
        Position::update_by_entity_id(&id, position);
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
#[spacetimedb(reducer)]
pub fn update_position_with_velocity(expected: u32) {
    let mut count = 0;
    for velocity in Velocity::iter() {
        let Some(mut position) = Position::filter_by_entity_id(&velocity.entity_id) else {
            continue;
        };

        position.x += velocity.x;
        position.y += velocity.y;
        position.z += velocity.z;

        let id = position.entity_id;
        Position::update_by_entity_id(&id, position);
        count += 1;
    }
    log::info!("UPDATE POSITION BY VELOCITY: {expected}, processed: {count}");
}

// Simulations for a game loop

#[spacetimedb(reducer)]
pub fn insert_world(players: u64) {
    for (i, id) in (0..players).enumerate() {
        let next_action_timestamp = if i & 2 == 2 {
            moment_milliseconds() + 2000 // Check every 2secs
        } else {
            moment_milliseconds()
        };

        GameEnemyAiAgentState::insert(GameEnemyAiAgentState {
            entity_id: id,
            next_action_timestamp,
            last_move_timestamps: vec![id, 0, id * 2],
            action: AgentAction::Idle,
        })
        .unwrap();

        GameLiveTargetableState::insert(GameLiveTargetableState {
            entity_id: id,
            quad: id as i64,
        })
        .unwrap();

        GameTargetableState::insert(GameTargetableState {
            entity_id: id,
            quad: id as i64,
        })
        .unwrap();

        GameMobileEntityState::insert(GameMobileEntityState {
            entity_id: id,
            location_x: id as i32,
            location_y: id as i32,
            timestamp: next_action_timestamp,
        })
        .unwrap();

        GameEnemyState::insert(GameEnemyState {
            entity_id: id,
            herd_id: id as i32,
        })
        .unwrap();

        GameHerdCache::insert(GameHerdCache {
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
        })
        .unwrap();
    }
    log::info!("INSERT WORLD PLAYERS: {players}");
}

fn get_targetables_near_quad(entity_id: u64, num_players: u64) -> Vec<GameTargetableState> {
    let mut result = Vec::with_capacity(4);

    for id in entity_id..num_players {
        for t in GameLiveTargetableState::filter_by_quad(&(id as i64)) {
            result.push(GameTargetableState::filter_by_entity_id(&t.entity_id).expect("Identity not found"))
        }
    }

    result
}

const MAX_MOVE_TIMESTAMPS: usize = 20;
fn move_agent(agent: &mut GameEnemyAiAgentState, agent_coord: SmallHexTile, current_time_ms: u64) {
    let entity_id = agent.entity_id;

    let enemy = GameEnemyState::filter_by_entity_id(&entity_id)
        .expect("GameEnemyState Entity ID not found")
        .clone();
    GameEnemyState::update_by_entity_id(&entity_id, enemy);

    agent.next_action_timestamp = current_time_ms + 2000;

    // Keep track of the last [MAX_MOVE_TIMESTAMPS] movements
    agent.last_move_timestamps.push(current_time_ms);
    if agent.last_move_timestamps.len() > MAX_MOVE_TIMESTAMPS {
        agent.last_move_timestamps.remove(0);
    }

    // Update targetable to the destination
    let mut targetable =
        GameTargetableState::filter_by_entity_id(&entity_id).expect("GameTargetableState Entity ID not found");
    let new_hash = calculate_hash(&targetable.quad) as i64;
    targetable.quad = new_hash;
    GameTargetableState::update_by_entity_id(&entity_id, targetable);

    // If the entity is alive (which it should be),
    // also update the `LiveTargetableState` used by `enemy_ai_agent_loop`.
    if GameLiveTargetableState::filter_by_entity_id(&entity_id).is_some() {
        GameLiveTargetableState::update_by_entity_id(
            &entity_id,
            GameLiveTargetableState {
                entity_id,
                quad: new_hash,
            },
        );
    }
    let mobile_entity =
        GameMobileEntityState::filter_by_entity_id(&entity_id).expect("GameMobileEntityState Entity ID not found");
    let mobile_entity = GameMobileEntityState {
        entity_id,
        location_x: mobile_entity.location_x + 1,
        location_y: mobile_entity.location_y + 1,
        timestamp: moment_milliseconds(),
    };

    GameEnemyAiAgentState::update_by_entity_id(&entity_id, agent.clone());

    GameMobileEntityState::update_by_entity_id(&entity_id, mobile_entity);
}

fn agent_loop(
    mut agent: GameEnemyAiAgentState,
    agent_targetable: GameTargetableState,
    surrounding_agents: &[GameTargetableState],
    current_time_ms: u64,
) {
    let entity_id = agent.entity_id;
    let coordinates =
        GameMobileEntityState::filter_by_entity_id(&entity_id).expect("GameMobileEntityState Entity ID not found");

    let agent_entity = GameEnemyState::filter_by_entity_id(&entity_id).expect("GameEnemyState Entity ID not found");
    let agent_herd = GameHerdCache::filter_by_id(&agent_entity.herd_id).expect("GameHerdCache Entity ID not found");
    let agent_herd_coordinates = agent_herd.location;

    move_agent(&mut agent, agent_herd_coordinates, current_time_ms);
}

// We check only for a single pass in the game loop.
#[spacetimedb(reducer)]
pub fn game_loop_enemy_ia(players: u64) {
    let mut count = 0;
    let current_time_ms = moment_milliseconds();

    for mut agent in GameEnemyAiAgentState::iter() {
        if agent.next_action_timestamp > current_time_ms {
            continue;
        }

        let agent_targetable = GameTargetableState::filter_by_entity_id(&agent.entity_id)
            .expect("No TargetableState for AgentState entity");

        let surrounding_agents = get_targetables_near_quad(agent_targetable.entity_id, players);

        agent.action = AgentAction::Fighting;

        agent_loop(agent, agent_targetable, &surrounding_agents, current_time_ms);

        count += 1;
    }

    log::info!("ENEMY IA LOOP PLAYERS: {players}, processed: {count}");
}

#[spacetimedb(reducer)]
pub fn init_game_ia_loop(initial_load: u32) {
    let load = Load::new(initial_load);

    insert_bulk_position(load.biggest_table);
    insert_bulk_velocity(load.big_table);
    update_position_all(load.biggest_table);
    update_position_with_velocity(load.big_table);

    insert_world(load.num_players as u64);
}

#[spacetimedb(reducer)]
pub fn run_game_ia_loop(initial_load: u32) {
    let load = Load::new(initial_load);

    game_loop_enemy_ia(load.num_players as u64);
}
