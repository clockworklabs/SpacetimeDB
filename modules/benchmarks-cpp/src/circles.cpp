//! STDB module used for benchmarks based on "realistic" workloads we are focusing in improving.
//! Circles benchmark - Game-like entities with spatial queries

#include "common.h"
#include <cmath>
#include <algorithm>

// =============================================================================
// CIRCLES BENCHMARK - DATA STRUCTURES
// =============================================================================

// Vector2 - 2D coordinate structure
struct Vector2 {
    float x;
    float y;
};
SPACETIMEDB_STRUCT(Vector2, x, y)

// Entity table - represents game objects with position and mass
struct Entity {
    uint32_t id;
    Vector2 position;
    uint32_t mass;
};
SPACETIMEDB_STRUCT(Entity, id, position, mass)
SPACETIMEDB_TABLE(Entity, entity, Public)
FIELD_PrimaryKeyAutoInc(entity, id)

// Circle table - represents player-controlled entities
struct Circle {
    uint32_t entity_id;
    uint32_t player_id;
    Vector2 direction;
    float magnitude;
    Timestamp last_split_time;
};
SPACETIMEDB_STRUCT(Circle, entity_id, player_id, direction, magnitude, last_split_time)
SPACETIMEDB_TABLE(Circle, circle, Public)
FIELD_PrimaryKey(circle, entity_id)
FIELD_Index(circle, player_id)

// Food table - represents consumable game objects
struct Food {
    uint32_t entity_id;
};
SPACETIMEDB_STRUCT(Food, entity_id)
SPACETIMEDB_TABLE(Food, food, Public)
FIELD_PrimaryKey(food, entity_id)

// =============================================================================
// CIRCLES BENCHMARK - HELPER FUNCTIONS
// =============================================================================

// Convert mass to radius for collision detection
inline float mass_to_radius(uint32_t mass) {
    return std::sqrt(static_cast<float>(mass));
}

// Check if two entities are overlapping based on their positions and masses
inline bool is_overlapping(const Entity& entity1, const Entity& entity2) {
    float entity1_radius = mass_to_radius(entity1.mass);
    float entity2_radius = mass_to_radius(entity2.mass);
    float dx = entity1.position.x - entity2.position.x;
    float dy = entity1.position.y - entity2.position.y;
    float distance = std::sqrt(dx * dx + dy * dy);
    return distance < std::max(entity1_radius, entity2_radius);
}

// =============================================================================
// CIRCLES BENCHMARK - BULK INSERT OPERATIONS
// =============================================================================

// Bulk insert entities with auto-incremented IDs
SPACETIMEDB_REDUCER(insert_bulk_entity, ReducerContext& ctx, uint32_t count) {
    for (uint32_t id = 0; id < count; ++id) {
        Entity new_entity = {
            0, // Auto-incremented by database
            {static_cast<float>(id), static_cast<float>(id + 5)}, // position
            id * 5 // mass
        };
        ctx.db[entity].insert(new_entity);
    }
    LOG_INFO("INSERT ENTITY: " + std::to_string(count));
    return Ok();
}

// Bulk insert circles with specified entity and player IDs
SPACETIMEDB_REDUCER(insert_bulk_circle, ReducerContext& ctx, uint32_t count) {
    for (uint32_t id = 0; id < count; ++id) {
        Circle new_circle = {
            id, // entity_id
            id, // player_id
            {static_cast<float>(id), static_cast<float>(id + 5)}, // direction
            static_cast<float>(id * 5), // magnitude
            ctx.timestamp // last_split_time
        };
        ctx.db[circle].insert(new_circle);
    }
    LOG_INFO("INSERT CIRCLE: " + std::to_string(count));
    return Ok();
}

// Bulk insert food entities
SPACETIMEDB_REDUCER(insert_bulk_food, ReducerContext& ctx, uint32_t count) {
    for (uint32_t id = 1; id <= count; ++id) {
        Food new_food = {id};
        ctx.db[food].insert(new_food);
    }
    LOG_INFO("INSERT FOOD: " + std::to_string(count));
    return Ok();
}

// =============================================================================
// CIRCLES BENCHMARK - CROSS JOIN OPERATIONS
// =============================================================================

// Simulate: SELECT * FROM Circle, Entity, Food
// This creates a Cartesian product of all three tables
SPACETIMEDB_REDUCER(cross_join_all, ReducerContext& ctx, uint32_t expected) {
    uint32_t count = 0;
    for (const auto& _circle : ctx.db[circle]) {
        for (const auto& _entity : ctx.db[entity]) {
            for (const auto& _food : ctx.db[food]) {
                ++count;
            }
        }
    }
    LOG_INFO("CROSS JOIN ALL: " + std::to_string(expected) + ", processed: " + std::to_string(count));
    return Ok();
}

// Simulate: SELECT * FROM Circle JOIN Entity USING(entity_id), Food JOIN Entity USING(entity_id)
// This joins circles with their entities, then cross-joins with food entities to check overlaps
SPACETIMEDB_REDUCER(cross_join_circle_food, ReducerContext& ctx, uint32_t expected) {
    uint32_t count = 0;
    for (const auto& circle_elem : ctx.db[circle]) {
        // Find the entity for this circle
        auto circle_entity_opt = ctx.db[entity_id].find(circle_elem.entity_id);
        if (!circle_entity_opt) {
            continue; // Skip if entity not found
        }
        const auto& circle_entity = *circle_entity_opt;
        
        // Cross join with all food entities
        for (const auto& food_elem : ctx.db[food]) {
            ++count;
            // Find the entity for this food
            auto food_entity_opt = ctx.db[entity_id].find(food_elem.entity_id);
            if (!food_entity_opt) {
                return Err("Entity not found: " + std::to_string(food_elem.entity_id));
            }
            const auto& food_entity = *food_entity_opt;
            
            // Check overlap and use black_box to prevent optimization
            black_box(is_overlapping(circle_entity, food_entity));
        }
    }
    LOG_INFO("CROSS JOIN CIRCLE FOOD: " + std::to_string(expected) + ", processed: " + std::to_string(count));
    return Ok();
}

// =============================================================================
// CIRCLES BENCHMARK - GAME SIMULATION ENTRY POINTS
// =============================================================================

// Initialize the circles game simulation with test data
SPACETIMEDB_REDUCER(init_game_circles, ReducerContext& ctx, uint32_t initial_load) {
    Load load(initial_load);
    
    // Set up the game world with food, entities, and circles
    auto bulk_food_res = insert_bulk_food(ctx, load.initial_load);
    if (bulk_food_res.is_err()) {
        return bulk_food_res;
    }
    auto bulk_entity_res = insert_bulk_entity(ctx, load.initial_load);
    if (bulk_entity_res.is_err()) {
        return bulk_entity_res;
    }
    auto bulk_circle_res = insert_bulk_circle(ctx, load.small_table);
    if (bulk_circle_res.is_err()) {
        return bulk_circle_res;
    }
    return Ok();
}

// Run the circles game simulation benchmark
SPACETIMEDB_REDUCER(run_game_circles, ReducerContext& ctx, uint32_t initial_load) {
    Load load(initial_load);
    
    // Perform the main benchmark operations
    auto cross_join_circle_food_res = cross_join_circle_food(ctx, initial_load * load.small_table);
    if (cross_join_circle_food_res.is_err()) {
        return cross_join_circle_food_res;
    }
    auto cross_join_all_res = cross_join_all(ctx, initial_load * initial_load * load.small_table);
    if (cross_join_all_res.is_err()) {
        return cross_join_all_res;
    }
    return Ok();
}

