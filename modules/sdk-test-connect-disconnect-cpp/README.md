# sdk-test-view-cpp

C++ implementation of the SpacetimeDB view test module. This mirrors the Rust `sdk-test-view` module to ensure feature parity between the C++ and Rust SDKs.

## Overview

This module tests the C++ bindings's view functionality including:

- **ViewContext views** - Views with caller identity
- **AnonymousViewContext views** - Views without caller identity  
- **Optional return types** - `std::optional<T>`
- **Vector return types** - `std::vector<T>`
- **Table joins** - Combining data from multiple tables
- **Filtering** - Using indexed fields for efficient queries
- **Complex queries** - Multi-table joins with filtering logic

## Tables

- `player` - Player entities with unique identities
- `player_level` - Player levels indexed for filtering
- `player_location` - Player positions with active status

## Views

### my_player
Returns the caller's player record.
- **Context**: ViewContext (requires sender)
- **Returns**: `std::optional<Player>`

### my_player_and_level  
Returns the caller's player joined with their level.
- **Context**: ViewContext (requires sender)
- **Returns**: `std::optional<PlayerAndLevel>`

### players_at_level_0
Returns all players at level 0.
- **Context**: AnonymousViewContext (no sender needed)
- **Returns**: `std::vector<Player>`

### nearby_players
Returns all active players within 5 units of the caller.
- **Context**: ViewContext (requires sender)
- **Returns**: `std::vector<PlayerLocation>`

## Building

```bash
emcmake cmake -B build
cmake --build build
```

The output will be `build/lib.wasm`.
