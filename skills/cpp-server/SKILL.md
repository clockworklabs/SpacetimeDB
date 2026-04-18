---
name: cpp-server
description: SpacetimeDB C++ server module SDK reference. Use when writing tables, reducers, or module logic in C++.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: server
  language: cpp
  cursor_globs: "**/*.cpp,**/*.h,**/*.hpp"
  cursor_always_apply: true
---

# SpacetimeDB C++ SDK Reference

## Imports

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;
```

## Tables

Register structs with macros, then declare as tables:

```cpp
struct Entity {
    uint64_t id;
    Identity owner;
    std::string name;
    bool active;
};
SPACETIMEDB_STRUCT(Entity, id, owner, name, active)
SPACETIMEDB_TABLE(Entity, entity, Public)
FIELD_PrimaryKeyAutoInc(entity, id)
FIELD_Index(entity, name)
```

Options: `SPACETIMEDB_TABLE(Type, accessor, Public|Private)`

Field constraints:
- `FIELD_PrimaryKey(accessor, field)` — primary key
- `FIELD_PrimaryKeyAutoInc(accessor, field)` — primary key with auto-increment (use 0 on insert)
- `FIELD_Unique(accessor, field)` — unique constraint
- `FIELD_Index(accessor, field)` — btree index (enables `.filter()`)

## Column Types

| C++ type | Notes |
|----------|-------|
| `uint8_t` / `uint16_t` / `uint32_t` / `uint64_t` | unsigned integers |
| `SpacetimeDB::u128` / `SpacetimeDB::u256` | large unsigned integers |
| `int8_t` / `int16_t` / `int32_t` / `int64_t` | signed integers |
| `SpacetimeDB::i128` / `SpacetimeDB::i256` | large signed integers |
| `float` / `double` | floats |
| `bool` | boolean |
| `std::string` | text |
| `std::vector<T>` | list/array |
| `std::optional<T>` | nullable column |
| `Identity` | user identity |
| `ConnectionId` | connection handle |
| `Timestamp` | server timestamp (microseconds since epoch) |
| `TimeDuration` | duration in microseconds |
| `ScheduleAt` | for scheduled tables |

## Indexes

```cpp
// Single-column:
FIELD_Index(entity, name)
// Access: ctx.db[entity_name].filter("Alice")

// Multi-column:
FIELD_NamedMultiColumnIndex(score, by_player_and_level, player_id, level)
```

Range queries (requires `#include <spacetimedb/range_queries.h>`):
```cpp
ctx.db[user_age].filter(range_inclusive(uint8_t(18), uint8_t(65)));
ctx.db[user_age].filter(range_from(uint8_t(18)));
```

## Reducers

All reducers return `ReducerResult` — use `Ok()` or `Err(message)`:

```cpp
SPACETIMEDB_REDUCER(create_entity, ReducerContext ctx, std::string name) {
    if (name.empty()) {
        return Err("Name cannot be empty");
    }
    ctx.db[entity].insert(Entity{0, ctx.sender(), name, true});
    return Ok();
}
```

## DB Operations

```cpp
ctx.db[entity].insert(Entity{0, owner, "Sample", true});     // Insert (0 for autoInc)
ctx.db[entity_id].find(entityId);                             // Find by PK → std::optional
ctx.db[entity_identity].find(ctx.sender());                   // Find by unique column
ctx.db[entity_name].filter("Alice");                          // Filter by index → iterable
ctx.db[entity];                                               // All rows → iterable (range-for)
ctx.db[entity].count();                                       // Count rows

// Update: find, mutate, update
if (auto e = ctx.db[entity_id].find(entityId)) {
    e->name = "New Name";
    ctx.db[entity_id].update(*e);
}

// Delete by primary key
ctx.db[entity_id].delete_by_key(entityId);
```

Note: Bracket notation `ctx.db[accessor]` is used for all table access. The accessor name comes from `SPACETIMEDB_TABLE` and `FIELD_*` macros.

## Lifecycle Hooks

```cpp
SPACETIMEDB_INIT(init, ReducerContext ctx) {
    LOG_INFO("Database initializing...");
    return Ok();
}

SPACETIMEDB_CLIENT_CONNECTED(on_connect, ReducerContext ctx) {
    LOG_INFO("Connected: " + ctx.sender().to_string());
    return Ok();
}

SPACETIMEDB_CLIENT_DISCONNECTED(on_disconnect, ReducerContext ctx) {
    LOG_INFO("Disconnected: " + ctx.sender().to_string());
    return Ok();
}
```

## Authentication & Timestamps

```cpp
// Auth: ctx.sender() is the caller's Identity
if (row.owner != ctx.sender()) {
    return Err("unauthorized");
}

// Server timestamps
ctx.db[item].insert(Item{0, ctx.sender(), ctx.timestamp});

// Timestamp arithmetic
Timestamp later = ctx.timestamp + TimeDuration::from_seconds(10);
```

## Reducer Context

```cpp
ctx.db[table]          // Table access (bracket notation)
ctx.sender()           // Caller's Identity
ctx.timestamp          // Invocation timestamp
ctx.connection_id      // std::optional<ConnectionId>
ctx.identity()         // Module's own identity
ctx.rng()              // Deterministic RNG
ctx.sender_auth()      // AuthCtx with JWT claims
```

## Scheduled Tables

```cpp
struct Reminder {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    std::string message;
};
SPACETIMEDB_STRUCT(Reminder, scheduled_id, scheduled_at, message)
SPACETIMEDB_TABLE(Reminder, reminder, Public)
FIELD_PrimaryKeyAutoInc(reminder, scheduled_id)
SPACETIMEDB_SCHEDULE(reminder, 1, send_reminder)  // 1 = scheduled_at field index (0-based)

SPACETIMEDB_REDUCER(send_reminder, ReducerContext ctx, Reminder arg) {
    LOG_INFO("Reminder: " + arg.message);
    return Ok();
}

// One-time: fires at a specific time
ctx.db[reminder].insert(Reminder{0, ScheduleAt::time(ctx.timestamp + TimeDuration::from_seconds(10)), "msg"});
// Repeating: fires on an interval
ctx.db[reminder].insert(Reminder{0, ScheduleAt::interval(TimeDuration::from_seconds(5)), "msg"});
```

## Custom Types

```cpp
// Struct (product type):
struct Point { float x; float y; };
SPACETIMEDB_STRUCT(Point, x, y)

// Enum (sum type):
SPACETIMEDB_UNIT_TYPE(Active)
SPACETIMEDB_UNIT_TYPE(Inactive)
SPACETIMEDB_ENUM(PlayerStatus,
    (Active, Active),
    (Inactive, Inactive),
    (Suspended, std::string)
)
```

## Logging

```cpp
LOG_INFO("Message: " + msg);
LOG_WARN("Warning: " + msg);
LOG_ERROR("Error: " + msg);
LOG_DEBUG("Debug: " + msg);
LOG_PANIC("Fatal: " + msg);   // terminates reducer
```

## Complete Example

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

struct Entity {
    Identity identity;
    std::string name;
    bool active;
};
SPACETIMEDB_STRUCT(Entity, identity, name, active)
SPACETIMEDB_TABLE(Entity, entity, Public)
FIELD_PrimaryKey(entity, identity)

struct Record {
    uint64_t id;
    Identity owner;
    uint32_t value;
    Timestamp created_at;
};
SPACETIMEDB_STRUCT(Record, id, owner, value, created_at)
SPACETIMEDB_TABLE(Record, record, Public)
FIELD_PrimaryKeyAutoInc(record, id)

SPACETIMEDB_CLIENT_CONNECTED(on_connect, ReducerContext ctx) {
    if (auto existing = ctx.db[entity_identity].find(ctx.sender())) {
        existing->active = true;
        ctx.db[entity_identity].update(*existing);
    }
    return Ok();
}

SPACETIMEDB_CLIENT_DISCONNECTED(on_disconnect, ReducerContext ctx) {
    if (auto existing = ctx.db[entity_identity].find(ctx.sender())) {
        existing->active = false;
        ctx.db[entity_identity].update(*existing);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(create_entity, ReducerContext ctx, std::string name) {
    if (ctx.db[entity_identity].find(ctx.sender())) {
        return Err("already exists");
    }
    ctx.db[entity].insert(Entity{ctx.sender(), name, true});
    return Ok();
}

SPACETIMEDB_REDUCER(add_record, ReducerContext ctx, uint32_t value) {
    if (!ctx.db[entity_identity].find(ctx.sender())) {
        return Err("not found");
    }
    ctx.db[record].insert(Record{0, ctx.sender(), value, ctx.timestamp});
    return Ok();
}
```
