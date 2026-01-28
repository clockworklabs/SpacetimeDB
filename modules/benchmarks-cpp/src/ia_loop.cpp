//! STDB module used for benchmarks based on "realistic" workloads we are focusing in improving.
//! IA Loop benchmark - AI agent simulation with complex state management

#include "common.h"
#include <cmath>
#include <algorithm>
#include <functional>
#include <vector>

// =============================================================================
// IA_LOOP BENCHMARK - DATA STRUCTURES
// =============================================================================

// Velocity table for entity movement
struct Velocity {
    uint32_t entity_id;
    float x;
    float y;
    float z;
};
SPACETIMEDB_STRUCT(Velocity, entity_id, x, y, z)
SPACETIMEDB_TABLE(Velocity, velocity, Public)
FIELD_PrimaryKey(velocity, entity_id)

// Position table with extended velocity fields
struct Position {
    uint32_t entity_id;
    float x;
    float y;
    float z;
    float vx;
    float vy;
    float vz;
};
SPACETIMEDB_STRUCT(Position, entity_id, x, y, z, vx, vy, vz)
SPACETIMEDB_TABLE(Position, position, Public)
FIELD_PrimaryKey(position, entity_id)

// Agent action enumeration
SPACETIMEDB_ENUM(AgentAction, Inactive, Idle, Evading, Investigating, Retreating, Fighting)

// AI agent state management
struct GameEnemyAiAgentState {
    uint64_t entity_id;
    std::vector<uint64_t> last_move_timestamps;
    uint64_t next_action_timestamp;
    AgentAction action;
};
SPACETIMEDB_STRUCT(GameEnemyAiAgentState, entity_id, last_move_timestamps, next_action_timestamp, action)
SPACETIMEDB_TABLE(GameEnemyAiAgentState, game_enemy_ai_agent_state, Public)
FIELD_PrimaryKey(game_enemy_ai_agent_state, entity_id)

// Targetable state for spatial queries
struct GameTargetableState {
    uint64_t entity_id;
    int64_t quad;
};
SPACETIMEDB_STRUCT(GameTargetableState, entity_id, quad)
SPACETIMEDB_TABLE(GameTargetableState, game_targetable_state, Public)
FIELD_PrimaryKey(game_targetable_state, entity_id)

// Live targetable state with quad indexing
struct GameLiveTargetableState {
    uint64_t entity_id;
    int64_t quad;
};
SPACETIMEDB_STRUCT(GameLiveTargetableState, entity_id, quad)
SPACETIMEDB_TABLE(GameLiveTargetableState, game_live_targetable_state, Public)
FIELD_Unique(game_live_targetable_state, entity_id)
FIELD_Index(game_live_targetable_state, quad)

// Mobile entity state with spatial indexing
struct GameMobileEntityState {
    uint64_t entity_id;
    int32_t location_x;
    int32_t location_y;
    uint64_t timestamp;
};
SPACETIMEDB_STRUCT(GameMobileEntityState, entity_id, location_x, location_y, timestamp)
SPACETIMEDB_TABLE(GameMobileEntityState, game_mobile_entity_state, Public)
FIELD_PrimaryKey(game_mobile_entity_state, entity_id)
FIELD_Index(game_mobile_entity_state, location_x)

// Enemy state for herd management
struct GameEnemyState {
    uint64_t entity_id;
    int32_t herd_id;
};
SPACETIMEDB_STRUCT(GameEnemyState, entity_id, herd_id)
SPACETIMEDB_TABLE(GameEnemyState, game_enemy_state, Public)
FIELD_PrimaryKey(game_enemy_state, entity_id)

// Small hex tile coordinate structure
struct SmallHexTile {
    int32_t x;
    int32_t z;
    uint32_t dimension;
};
SPACETIMEDB_STRUCT(SmallHexTile, x, z, dimension)

// Herd cache for AI behavior
struct GameHerdCache {
    int32_t id;
    uint32_t dimension_id;
    int32_t current_population;
    SmallHexTile location;
    int32_t max_population;
    float spawn_eagerness;
    int32_t roaming_distance;
};
SPACETIMEDB_STRUCT(GameHerdCache, id, dimension_id, current_population, location, max_population, spawn_eagerness, roaming_distance)
SPACETIMEDB_TABLE(GameHerdCache, game_herd_cache, Public)
FIELD_PrimaryKey(game_herd_cache, id)

// =============================================================================
// IA_LOOP BENCHMARK - HELPER FUNCTIONS
// =============================================================================

// Simplified moment calculation - always returns 1 as per original implementations
inline uint64_t moment_milliseconds() {
    return 1;
}

// Simple hash calculation for quad values
inline uint64_t calculate_hash(int64_t value) {
    return static_cast<uint64_t>(std::hash<int64_t>{}(value));
}

// =============================================================================
// IA_LOOP BENCHMARK - POSITION AND VELOCITY OPERATIONS
// =============================================================================

// Bulk insert position entries
SPACETIMEDB_REDUCER(insert_bulk_position, ReducerContext& ctx, uint32_t count) {
    for (uint32_t id = 0; id < count; ++id) {
        float x = static_cast<float>(id);
        float y = static_cast<float>(id + 5);
        float z = static_cast<float>(id * 5);
        Position new_position = {
            id, x, y, z,
            x + 10.0f, y + 20.0f, z + 30.0f // vx, vy, vz
        };
        ctx.db[position].insert(new_position);
    }
    LOG_INFO("INSERT POSITION: " + std::to_string(count));
    return Ok();
}

// Bulk insert velocity entries
SPACETIMEDB_REDUCER(insert_bulk_velocity, ReducerContext& ctx, uint32_t count) {
    for (uint32_t id = 0; id < count; ++id) {
        Velocity new_velocity = {
            id,
            static_cast<float>(id),
            static_cast<float>(id + 5),
            static_cast<float>(id * 5)
        };
        ctx.db[velocity].insert(new_velocity);
    }
    LOG_INFO("INSERT VELOCITY: " + std::to_string(count));
    return Ok();
}

// Update all positions using their internal velocity
SPACETIMEDB_REDUCER(update_position_all, ReducerContext& ctx, uint32_t expected) {
    uint32_t count = 0;
    for (auto pos : ctx.db[position]) {
        pos.x += pos.vx;
        pos.y += pos.vy;
        pos.z += pos.vz;
        
        auto _updated = ctx.db[position_entity_id].update(pos);
        ++count;
    }
    LOG_INFO("UPDATE POSITION ALL: " + std::to_string(expected) + ", processed: " + std::to_string(count));
    return Ok();
}

// Update positions using separate velocity table
SPACETIMEDB_REDUCER(update_position_with_velocity, ReducerContext& ctx, uint32_t expected) {
    uint32_t count = 0;
    for (const auto& vel : ctx.db[velocity]) {
        auto pos_opt = ctx.db[position_entity_id].find(vel.entity_id);
        if (!pos_opt) {
            continue;
        }
        auto pos = *pos_opt;
        
        pos.x += vel.x;
        pos.y += vel.y;
        pos.z += vel.z;

        auto _updated = ctx.db[position_entity_id].update(pos);
        ++count;
    }
    LOG_INFO("UPDATE POSITION BY VELOCITY: " + std::to_string(expected) + ", processed: " + std::to_string(count));
    return Ok();
}

// =============================================================================
// IA_LOOP BENCHMARK - WORLD SETUP
// =============================================================================

// Insert complete game world state for specified number of players
SPACETIMEDB_REDUCER(insert_world, ReducerContext& ctx, uint64_t players) {
    for (uint64_t i = 0; i < players; ++i) {
        uint64_t next_action_timestamp = (i & 2) == 2 ? 
            moment_milliseconds() + 2000 : moment_milliseconds();
        
        // Insert AI agent state
        std::vector<uint64_t> move_timestamps = {i, 0, i * 2};
        GameEnemyAiAgentState agent_state = {
            i, move_timestamps, next_action_timestamp, AgentAction::Idle
        };
        ctx.db[game_enemy_ai_agent_state].insert(agent_state);
        
        // Insert live targetable state
        GameLiveTargetableState live_targetable = {
            i, static_cast<int64_t>(i)
        };
        ctx.db[game_live_targetable_state].insert(live_targetable);
        
        // Insert targetable state
        GameTargetableState targetable = {
            i, static_cast<int64_t>(i)
        };
        ctx.db[game_targetable_state].insert(targetable);
        
        // Insert mobile entity state
        GameMobileEntityState mobile_entity = {
            i, static_cast<int32_t>(i), static_cast<int32_t>(i), next_action_timestamp
        };
        ctx.db[game_mobile_entity_state].insert(mobile_entity);
        
        // Insert enemy state
        GameEnemyState enemy = {
            i, static_cast<int32_t>(i)
        };
        ctx.db[game_enemy_state].insert(enemy);
        
        // Insert herd cache
        SmallHexTile tile = {
            static_cast<int32_t>(i),
            static_cast<int32_t>(i),
            static_cast<uint32_t>(i * 2)
        };
        GameHerdCache herd = {
            static_cast<int32_t>(i),
            static_cast<uint32_t>(i),
            static_cast<int32_t>(i * 2),
            tile,
            static_cast<int32_t>(i * 4),
            static_cast<float>(i),
            static_cast<int32_t>(i)
        };
        ctx.db[game_herd_cache].insert(herd);
    }
    LOG_INFO("INSERT WORLD PLAYERS: " + std::to_string(players));
    return Ok();
}

// =============================================================================
// IA_LOOP BENCHMARK - GAME LOGIC
// =============================================================================

// Get targetable entities near a specific quad
std::vector<GameTargetableState> get_targetables_near_quad(
    ReducerContext& ctx, uint64_t entity_id, uint64_t num_players) {
    std::vector<GameTargetableState> result;
    result.reserve(4);
    
    for (uint64_t id = entity_id; id < num_players; ++id) {
        int64_t quad = static_cast<int64_t>(id);
        for (const auto& t : ctx.db[game_live_targetable_state_quad].filter(quad)) {
            auto targetable_opt = ctx.db[game_targetable_state_entity_id].find(t.entity_id);
            if (!targetable_opt) {
                LOG_PANIC("Identity not found");
                return result;
            }
            result.push_back(*targetable_opt);
        }
    }
    
    return result;
}

const std::size_t MAX_MOVE_TIMESTAMPS = 20;

// Move agent logic - updates agent state and related entities
void move_agent(ReducerContext& ctx, GameEnemyAiAgentState& agent, 
                const SmallHexTile& agent_coord, uint64_t current_time_ms) {
    uint64_t entity_id = agent.entity_id;
    
    // Update enemy state
    auto enemy_opt = ctx.db[game_enemy_state_entity_id].find(entity_id);
    if (!enemy_opt) {
        LOG_PANIC("GameEnemyState Entity ID not found");
        return;
    }
    auto enemy = *enemy_opt;
    auto _updated = ctx.db[game_enemy_state_entity_id].update(enemy);
    
    // Update agent timestamp
    agent.next_action_timestamp = current_time_ms + 2000;
    
    // Track movement timestamps
    agent.last_move_timestamps.push_back(current_time_ms);
    if (agent.last_move_timestamps.size() > MAX_MOVE_TIMESTAMPS) {
        agent.last_move_timestamps.erase(agent.last_move_timestamps.begin());
    }
    
    // Update targetable state
    auto targetable_opt = ctx.db[game_targetable_state_entity_id].find(entity_id);
    if (!targetable_opt) {
        LOG_PANIC("GameTargetableState Entity ID not found");
        return;
    }
    auto targetable = *targetable_opt;
    int64_t new_hash = static_cast<int64_t>(calculate_hash(targetable.quad));
    targetable.quad = new_hash;
    _updated = ctx.db[game_targetable_state_entity_id].update(targetable);
    
    // Update live targetable state if exists
    auto live_targetable_opt = ctx.db[game_live_targetable_state_entity_id].find(entity_id);
    if (live_targetable_opt) {
        GameLiveTargetableState live_targetable = {entity_id, new_hash};
        ctx.db[game_live_targetable_state_entity_id].update(live_targetable);
    }
    
    // Update mobile entity state
    auto mobile_entity_opt = ctx.db[game_mobile_entity_state_entity_id].find(entity_id);
    if (!mobile_entity_opt) {
        LOG_PANIC("GameMobileEntityState Entity ID not found");
        return;
    }
    auto mobile_entity = *mobile_entity_opt;
    mobile_entity.location_x += 1;
    mobile_entity.location_y += 1;
    mobile_entity.timestamp = agent.next_action_timestamp;
    
    // Update agent state and mobile entity
    _updated = ctx.db[game_enemy_ai_agent_state_entity_id].update(agent);
    _updated = ctx.db[game_mobile_entity_state_entity_id].update(mobile_entity);
}

// Main agent loop processing
void agent_loop(ReducerContext& ctx, GameEnemyAiAgentState& agent,
                const GameTargetableState& agent_targetable,
                const std::vector<GameTargetableState>& surrounding_agents,
                uint64_t current_time_ms) {
    uint64_t entity_id = agent.entity_id;
    
    auto coordinates_opt = ctx.db[game_mobile_entity_state_entity_id].find(entity_id);
    if (!coordinates_opt) {
        LOG_PANIC("GameMobileEntityState Entity ID not found");
        return;
    }
    
    auto agent_entity_opt = ctx.db[game_enemy_state_entity_id].find(entity_id);
    if (!agent_entity_opt) {
        LOG_PANIC("GameEnemyState Entity ID not found");
        return;
    }
    const auto& agent_entity = *agent_entity_opt;
    
    auto agent_herd_opt = ctx.db[game_herd_cache_id].find(agent_entity.herd_id);
    if (!agent_herd_opt) {
        LOG_PANIC("GameHerdCache Entity ID not found");
        return;
    }
    const auto& agent_herd = *agent_herd_opt;
    
    SmallHexTile agent_herd_coordinates = agent_herd.location;
    
    move_agent(ctx, agent, agent_herd_coordinates, current_time_ms);
}

// Main game loop for enemy AI processing
SPACETIMEDB_REDUCER(game_loop_enemy_ia, ReducerContext& ctx, uint64_t players) {
    uint32_t count = 0;
    uint64_t current_time_ms = moment_milliseconds();
    
    for (auto agent : ctx.db[game_enemy_ai_agent_state]) {
        auto agent_targetable_opt = ctx.db[game_targetable_state_entity_id].find(agent.entity_id);
        if (!agent_targetable_opt) {
            return Err("No TargetableState for AgentState entity");
        }
        const auto& agent_targetable = *agent_targetable_opt;
        
        auto surrounding_agents = get_targetables_near_quad(ctx, agent_targetable.entity_id, players);
        
        agent.action = AgentAction::Fighting;
        
        agent_loop(ctx, agent, agent_targetable, surrounding_agents, current_time_ms);
        
        ++count;
    }
    
    LOG_INFO("ENEMY IA LOOP PLAYERS: " + std::to_string(players) + ", processed: " + std::to_string(count));
    return Ok();
}

// =============================================================================
// IA_LOOP BENCHMARK - GAME SIMULATION ENTRY POINTS
// =============================================================================

// Initialize the IA loop game simulation with test data
SPACETIMEDB_REDUCER(init_game_ia_loop, ReducerContext& ctx, uint32_t initial_load) {
    Load load(initial_load);
    
    auto bulk_position_res = insert_bulk_position(ctx, load.biggest_table);
    if (bulk_position_res.is_err()) {
        return bulk_position_res;
    }
    auto bulk_velocity_res = insert_bulk_velocity(ctx, load.big_table);
    if (bulk_velocity_res.is_err()) {
        return bulk_velocity_res;
    }
    auto update_position_all_res = update_position_all(ctx, load.biggest_table);
    if (update_position_all_res.is_err()) {
        return update_position_all_res;
    }
    auto update_position_with_velocity_res = update_position_with_velocity(ctx, load.big_table);
    if (update_position_with_velocity_res.is_err()) {
        return update_position_with_velocity_res;
    }
    
    auto insert_world_res = insert_world(ctx, static_cast<uint64_t>(load.num_players));
    if (insert_world_res.is_err()) {
        return insert_world_res;
    }
    return Ok();
}

// Run the IA loop game simulation benchmark
SPACETIMEDB_REDUCER(run_game_ia_loop, ReducerContext& ctx, uint32_t initial_load) {
    Load load(initial_load);
    
    auto game_loop_enemy_ia_res = game_loop_enemy_ia(ctx, static_cast<uint64_t>(load.num_players));
    if (game_loop_enemy_ia_res.is_err()) {
        return game_loop_enemy_ia_res;
    }
    return Ok();
}