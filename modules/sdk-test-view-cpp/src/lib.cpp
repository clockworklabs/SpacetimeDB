#include <spacetimedb.h>
#include <optional>
#include <vector>
#include <cmath>

using namespace SpacetimeDb;

// =============================================================================
// C++ SDK View Test Module - Mirrors Rust sdk-test-view
// =============================================================================
//
// This module provides a complete C++ implementation of the Rust sdk-test-view
// module, testing all view functionality including:
// - ViewContext views (with sender identity)
// - AnonymousViewContext views (without sender)
// - Optional and Vec return types
// - Joins across multiple tables
// - Filtering and complex queries
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

SPACETIMEDB_REDUCER(insert_player, ReducerContext ctx, uint64_t level) {
    // Insert player and get the entity_id
    Player player_result = ctx.db[player].insert(Player{0, ctx.sender});
    
    // Insert player level
    ctx.db[player_level].insert(PlayerLevel{player_result.entity_id, level});
    
    return Ok();
}

SPACETIMEDB_REDUCER(delete_player, ReducerContext ctx) {
    // Find player by identity
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return Ok(); // Player doesn't exist, nothing to delete
    }
    
    uint64_t eid = player_opt->entity_id;
    
    // Delete from both tables
    ctx.db[player_entity_id].delete_by_value(eid);
    ctx.db[player_level_entity_id].delete_by_value(eid);
    
    return Ok();
}

SPACETIMEDB_REDUCER(move_player, ReducerContext ctx, int32_t dx, int32_t dy) {
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

SPACETIMEDB_INIT(init) {
    Identity alice = Identity{std::array<uint8_t, 32>{1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0}};
    Identity bob = Identity{std::array<uint8_t, 32>{2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0}};
    Identity charlie = Identity{std::array<uint8_t, 32>{3,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0}};
    Identity david = Identity{std::array<uint8_t, 32>{4,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0}};
    
    // Insert players
    Player p1 = ctx.db[player].insert(Player{0, alice});
    Player p2 = ctx.db[player].insert(Player{0, bob});
    Player p3 = ctx.db[player].insert(Player{0, charlie});
    Player p4 = ctx.db[player].insert(Player{0, david});
    
    // Insert player levels - Alice and Bob are level 0, Charlie is level 1, David is level 2
    ctx.db[player_level].insert(PlayerLevel{p1.entity_id, 0});
    ctx.db[player_level].insert(PlayerLevel{p2.entity_id, 0});
    ctx.db[player_level].insert(PlayerLevel{p3.entity_id, 1});
    ctx.db[player_level].insert(PlayerLevel{p4.entity_id, 2});
    
    // Insert player locations
    // Alice at (0, 0) - active
    ctx.db[player_location].insert(PlayerLocation{p1.entity_id, true, 0, 0});
    // Bob at (2, 3) - active (within 5 units of Alice)
    ctx.db[player_location].insert(PlayerLocation{p2.entity_id, true, 2, 3});
    // Charlie at (10, 10) - active (NOT within 5 units of Alice)
    ctx.db[player_location].insert(PlayerLocation{p3.entity_id, true, 10, 10});
    // David at (1, 1) - inactive (within 5 units but inactive)
    ctx.db[player_location].insert(PlayerLocation{p4.entity_id, false, 1, 1});

    return Ok();
}

// =============================================================================
// VIEWS
// =============================================================================

// View: my_player - Returns the player for the caller
SPACETIMEDB_VIEW(std::optional<Player>, my_player, Public, ViewContext ctx) {
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    return Ok(player_opt);
}

// View: my_player_and_level - Returns player with level joined
SPACETIMEDB_VIEW(std::optional<PlayerAndLevel>, my_player_and_level, Public, ViewContext ctx) {
    // Find the caller's player
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return Ok(std::optional<PlayerAndLevel>());
    }
    
    Player p = player_opt.value();
    
    // Find the player's level
    auto level_opt = ctx.db[player_level_entity_id].find(p.entity_id);
    if (!level_opt.has_value()) {
        return Ok(std::optional<PlayerAndLevel>());
    }
    
    // Combine into result
    PlayerAndLevel result{
        p.entity_id,
        p.identity,
        level_opt->level
    };
    
    return Ok(std::optional<PlayerAndLevel>(result));
}

// View: players_at_level_0 - Returns all players at level 0 (anonymous)
SPACETIMEDB_VIEW(std::vector<Player>, players_at_level_0, Public, AnonymousViewContext ctx) {
    std::vector<Player> results;
    
    // Find all players at level 0
    for (IndexIterator<PlayerLevel> iter = ctx.db[player_level_level].filter(0ULL); 
         iter != IndexIterator<PlayerLevel>(); ++iter) {
        const auto& lvl = *iter;
        auto player_opt = ctx.db[player_entity_id].find(lvl.entity_id);
        if (player_opt.has_value()) {
            results.push_back(player_opt.value());
        }
    }
    
    return Ok(results);
}

// View: nearby_players - Returns players within 5 units
SPACETIMEDB_VIEW(std::vector<PlayerLocation>, nearby_players, Public, ViewContext ctx) {
    std::vector<PlayerLocation> results;
    
    // Find the caller's player
    auto my_player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!my_player_opt.has_value()) {
        return Ok(results); // No player, return empty
    }
    
    // Find the caller's location
    auto my_loc_opt = ctx.db[player_location_entity_id].find(my_player_opt->entity_id);
    if (!my_loc_opt.has_value()) {
        return Ok(results); // No location, return empty
    }
    
    PlayerLocation my_loc = my_loc_opt.value();
    
    // Find all active players
    for (IndexIterator<PlayerLocation> iter = ctx.db[player_location_active].filter(true); 
         iter != IndexIterator<PlayerLocation>(); ++iter) {
        const auto& loc = *iter;
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
    
    return Ok(results);
}

// =============================================================================
// TEST REDUCERS - Call views and log results
// =============================================================================

SPACETIMEDB_REDUCER(test_my_player, ReducerContext ctx) {
    auto result = my_player(ViewContext{ctx.sender});
    if (result.is_ok()) {
        auto player_opt = result.value();
        if (player_opt.has_value()) {
            LOG_INFO("my_player found: entity_id=" + std::to_string(player_opt->entity_id));
        } else {
            LOG_INFO("my_player returned None");
        }
    } else {
        LOG_ERROR("my_player failed: " + result.error());
    }
    return Ok();
}

SPACETIMEDB_REDUCER(test_my_player_and_level, ReducerContext ctx) {
    auto result = my_player_and_level(ViewContext{ctx.sender});
    if (result.is_ok()) {
        auto data_opt = result.value();
        if (data_opt.has_value()) {
            LOG_INFO("my_player_and_level found: entity_id=" + std::to_string(data_opt->entity_id) + 
                     " level=" + std::to_string(data_opt->level));
        } else {
            LOG_INFO("my_player_and_level returned None");
        }
    } else {
        LOG_ERROR("my_player_and_level failed: " + result.error());
    }
    return Ok();
}

SPACETIMEDB_REDUCER(test_players_at_level_0, ReducerContext ctx) {
    auto result = players_at_level_0(AnonymousViewContext{});
    if (result.is_ok()) {
        auto players = result.value();
        LOG_INFO("players_at_level_0 found " + std::to_string(players.size()) + " players");
        for (const auto& p : players) {
            LOG_INFO("  - entity_id=" + std::to_string(p.entity_id));
        }
    } else {
        LOG_ERROR("players_at_level_0 failed: " + result.error());
    }
    return Ok();
}

SPACETIMEDB_REDUCER(test_nearby_players, ReducerContext ctx) {
    auto result = nearby_players(ViewContext{ctx.sender});
    if (result.is_ok()) {
        auto locations = result.value();
        LOG_INFO("nearby_players found " + std::to_string(locations.size()) + " nearby players");
        for (const auto& loc : locations) {
            LOG_INFO("  - entity_id=" + std::to_string(loc.entity_id) + 
                     " at (" + std::to_string(loc.x) + ", " + std::to_string(loc.y) + ")");
        }
    } else {
        LOG_ERROR("nearby_players failed: " + result.error());
    }
    return Ok();
}
