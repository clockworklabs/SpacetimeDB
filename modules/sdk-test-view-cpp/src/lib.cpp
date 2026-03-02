#include <spacetimedb.h>
#include <optional>
#include <vector>
#include <cmath>

using namespace SpacetimeDB;

// =============================================================================
// C++ bindings View Test Module - Mirrors Rust sdk-test-view
// =============================================================================
//
// This module provides a complete C++ implementation of the Rust sdk-test-view
// module, testing all view functionality including:
// - ViewContext views (with sender identity)
// - AnonymousViewContext views (without sender)
// - Optional and Vec return types
// - Joins across multiple tables
// - Filtering and complex queries
//
// NOTE: This module has NO INIT function. Test data is created dynamically
// by the test client via the reducers.
// =============================================================================

// Table: player
struct Player {
    uint64_t entity_id;
    Identity identity;
};
SPACETIMEDB_STRUCT(Player, entity_id, identity)
SPACETIMEDB_TABLE(Player, player, Public)
FIELD_PrimaryKey(player, entity_id);
FIELD_AutoInc(player, entity_id);
FIELD_Unique(player, identity);

// Table: player_level
struct PlayerLevel {
    uint64_t entity_id;
    uint64_t level;
};
SPACETIMEDB_STRUCT(PlayerLevel, entity_id, level)
SPACETIMEDB_TABLE(PlayerLevel, player_level, Public)
FIELD_Unique(player_level, entity_id);
FIELD_Index(player_level, level);

// Table: player_location
struct PlayerLocation {
    uint64_t entity_id;
    bool active;
    int32_t x;
    int32_t y;
};
SPACETIMEDB_STRUCT(PlayerLocation, entity_id, active, x, y)
SPACETIMEDB_TABLE(PlayerLocation, player_location, Public)
FIELD_Unique(player_location, entity_id);
FIELD_Index(player_location, active);

// Custom type for joined results
struct PlayerAndLevel {
    uint64_t entity_id;
    Identity identity;
    uint64_t level;
};
SPACETIMEDB_STRUCT(PlayerAndLevel, entity_id, identity, level)

// =============================================================================
// REDUCERS
// =============================================================================

SPACETIMEDB_REDUCER(insert_player, ReducerContext ctx, Identity identity, uint64_t level)
{
    // Insert player and get the entity_id
    Player player_result = ctx.db[player].insert(Player{0, identity});
    
    // Insert player level
    ctx.db[player_level].insert(PlayerLevel{player_result.entity_id, level});
    
    return Ok();
}

SPACETIMEDB_REDUCER(delete_player, ReducerContext ctx, Identity identity)
{
    // Find player by identity using index
    auto player_opt = ctx.db[player_identity].find(identity);
    if (player_opt.has_value()) {
        uint64_t eid = player_opt->entity_id;
        
        // Delete from both tables
        ctx.db[player_entity_id].delete_by_key(eid);
        ctx.db[player_level_entity_id].delete_by_value(eid);
    }
    
    return Ok();
}

SPACETIMEDB_REDUCER(move_player, ReducerContext ctx, int32_t dx, int32_t dy)
{
    // Find or create player
    auto my_player_opt = ctx.db[player_identity].find(ctx.sender);
    Player my_player;
    
    if (!my_player_opt.has_value()) {
        // Create new player
        my_player = ctx.db[player].insert(Player{0, ctx.sender});
    } else {
        my_player = my_player_opt.value();
    }
    
    // Find or create location
    auto loc_opt = ctx.db[player_location_entity_id].find(my_player.entity_id);
    
    if (loc_opt.has_value()) {
        // Update existing location
        PlayerLocation updated = loc_opt.value();
        updated.x += dx;
        updated.y += dy;
        ctx.db[player_location_entity_id].update(updated);
    } else {
        // Insert new location
        ctx.db[player_location].insert(PlayerLocation{
            my_player.entity_id,
            true,
            dx,
            dy
        });
    }
    
    return Ok();
}

// =============================================================================
// VIEWS
// =============================================================================

// View: my_player - Returns the player for the caller
SPACETIMEDB_VIEW(std::optional<Player>, my_player, Public, ViewContext ctx) {
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    return player_opt;
}

// View: my_player_and_level - Returns player with level joined
SPACETIMEDB_VIEW(std::optional<PlayerAndLevel>, my_player_and_level, Public, ViewContext ctx) {
    // Find the caller's player
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return std::optional<PlayerAndLevel>();
    }
    
    Player p = player_opt.value();
    
    // Find the player's level
    auto level_opt = ctx.db[player_level_entity_id].find(p.entity_id);
    if (!level_opt.has_value()) {
        return std::optional<PlayerAndLevel>();
    }
    
    // Combine into result
    PlayerAndLevel result{
        p.entity_id,
        p.identity,
        level_opt->level
    };
    
    return std::optional<PlayerAndLevel>(result);
}

// View: players_at_level_0 - Returns all players at level 0 (anonymous)
SPACETIMEDB_VIEW(std::vector<Player>, players_at_level_0, Public, AnonymousViewContext ctx) {
    std::vector<Player> results;
    
    // Find all players at level 0
    for (const auto& lvl : ctx.db[player_level_level].filter(0ULL)) {
        auto player_opt = ctx.db[player_entity_id].find(lvl.entity_id);
        if (player_opt.has_value()) {
            results.push_back(player_opt.value());
        }
    }
    
    return results;
}

// View: nearby_players - Returns players within 5 units
SPACETIMEDB_VIEW(std::vector<PlayerLocation>, nearby_players, Public, ViewContext ctx) {
    std::vector<PlayerLocation> results;
    
    // Find the caller's player
    auto my_player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!my_player_opt.has_value()) {
        return results; // No player, return empty
    }
    
    // Find the caller's location
    auto my_loc_opt = ctx.db[player_location_entity_id].find(my_player_opt->entity_id);
    if (!my_loc_opt.has_value()) {
        return results; // No location, return empty
    }
    
    PlayerLocation my_loc = my_loc_opt.value();
    
    // Find all active players
    for (const auto& loc : ctx.db[player_location_active].filter(true)) {
        // Skip self
        if (loc.entity_id == my_loc.entity_id) {
            continue;
        }
        
        // Check if within range
        int32_t dx = std::abs(loc.x - my_loc.x);
        int32_t dy = std::abs(loc.y - my_loc.y);
        
        if (dx < 5 && dy < 5) {
            results.push_back(loc);
        }
    }
    
    return results;
}

