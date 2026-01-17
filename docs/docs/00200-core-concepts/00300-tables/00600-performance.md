---
title: Performance Best Practices
slug: /tables/performance
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Follow these guidelines to optimize table performance in your SpacetimeDB modules.

## Use Indexes for Lookups

Generally prefer indexed lookups over full table scans:

✅ **Good - Using an index:**

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Fast: Uses unique index on name
ctx.db.player.name.filter('Alice')
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Fast: Uses unique index on name
ctx.Db.Player.Name.Filter("Alice")
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Fast: Uses unique index on name
ctx.db.player().name().filter("Alice")
```

</TabItem>
</Tabs>

❌ **Avoid - Full table scan:**

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Slow: Iterates through all rows
Array.from(ctx.db.player.iter())
  .find(p => p.name === 'Alice')
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Slow: Iterates through all rows
ctx.Db.Player.Iter()
    .FirstOrDefault(p => p.Name == "Alice")
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Slow: Iterates through all rows
ctx.db.player()
    .iter()
    .find(|p| p.name == "Alice")
```

</TabItem>
</Tabs>

Add indexes to columns you frequently filter or join on. See [Indexes](/tables/indexes) for details.

## Keep Tables Focused

Break large tables into smaller, more focused tables when appropriate:

**Instead of one large table:**

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const player = table(
  { name: 'player' },
  {
    id: t.u32(),
    name: t.string(),
    // Game state
    position_x: t.f32(),
    position_y: t.f32(),
    health: t.u32(),
    // Statistics (rarely accessed)
    total_kills: t.u32(),
    total_deaths: t.u32(),
    play_time_seconds: t.u64(),
    // Settings (rarely changed)
    audio_volume: t.f32(),
    graphics_quality: t.u8(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table]
public partial struct Player
{
    public uint Id;
    public string Name;
    // Game state
    public float PositionX;
    public float PositionY;
    public uint Health;
    // Statistics (rarely accessed)
    public uint TotalKills;
    public uint TotalDeaths;
    public ulong PlayTimeSeconds;
    // Settings (rarely changed)
    public float AudioVolume;
    public byte GraphicsQuality;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = player)]
pub struct Player {
    id: u32,
    name: String,
    // Game state
    position_x: f32,
    position_y: f32,
    health: u32,
    // Statistics (rarely accessed)
    total_kills: u32,
    total_deaths: u32,
    play_time_seconds: u64,
    // Settings (rarely changed)
    audio_volume: f32,
    graphics_quality: u8,
}
```

</TabItem>
</Tabs>

**Consider splitting into multiple tables:**

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const player = table(
  { name: 'player' },
  {
    id: t.u32().primaryKey(),
    name: t.string().unique(),
  }
);

const playerState = table(
  { name: 'player_state' },
  {
    player_id: t.u32().unique(),
    position_x: t.f32(),
    position_y: t.f32(),
    health: t.u32(),
  }
);

const playerStats = table(
  { name: 'player_stats' },
  {
    player_id: t.u32().unique(),
    total_kills: t.u32(),
    total_deaths: t.u32(),
    play_time_seconds: t.u64(),
  }
);

const playerSettings = table(
  { name: 'player_settings' },
  {
    player_id: t.u32().unique(),
    audio_volume: t.f32(),
    graphics_quality: t.u8(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    public uint Id;
    [SpacetimeDB.Unique]
    public string Name;
}

[SpacetimeDB.Table]
public partial struct PlayerState
{
    [SpacetimeDB.Unique]
    public uint PlayerId;
    public float PositionX;
    public float PositionY;
    public uint Health;
}

[SpacetimeDB.Table]
public partial struct PlayerStats
{
    [SpacetimeDB.Unique]
    public uint PlayerId;
    public uint TotalKills;
    public uint TotalDeaths;
    public ulong PlayTimeSeconds;
}

[SpacetimeDB.Table]
public partial struct PlayerSettings
{
    [SpacetimeDB.Unique]
    public uint PlayerId;
    public float AudioVolume;
    public byte GraphicsQuality;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = player)]
pub struct Player {
    #[primary_key]
    id: u32,
    #[unique]
    name: String,
}

#[spacetimedb::table(name = player_state)]
pub struct PlayerState {
    #[unique]
    player_id: u32,
    position_x: f32,
    position_y: f32,
    health: u32,
}

#[spacetimedb::table(name = player_stats)]
pub struct PlayerStats {
    #[unique]
    player_id: u32,
    total_kills: u32,
    total_deaths: u32,
    play_time_seconds: u64,
}

#[spacetimedb::table(name = player_settings)]
pub struct PlayerSettings {
    #[unique]
    player_id: u32,
    audio_volume: f32,
    graphics_quality: u8,
}
```

</TabItem>
</Tabs>

Benefits:
- Reduces data transferred to clients who don't need all fields
- Allows more targeted subscriptions
- Improves update performance by touching fewer rows
- Makes the schema easier to understand and maintain

## Choose Appropriate Types

Use the smallest integer type that fits your data range:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// If you only need 0-255, use u8 instead of u64
level: t.u8(),           // Not t.u64()
player_count: t.u16(),   // Not t.u64()
entity_id: t.u32(),      // Not t.u64()
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// If you only need 0-255, use byte instead of ulong
public byte Level;           // Not ulong
public ushort PlayerCount;   // Not ulong
public uint EntityId;        // Not ulong
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// If you only need 0-255, use u8 instead of u64
level: u8,           // Not u64
player_count: u16,   // Not u64
entity_id: u32,      // Not u64
```

</TabItem>
</Tabs>

This reduces:
- Memory usage
- Network bandwidth
- Storage requirements

## Consider Table Visibility

Private tables avoid unnecessary client synchronization overhead:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Public table - clients can subscribe and receive updates
const player = table(
  { name: 'player', public: true },
  { /* ... */ }
);

// Private table - only visible to module and owner
// Better for internal state, caches, or sensitive data
const internalState = table(
  { name: 'internal_state' },
  { /* ... */ }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Public table - clients can subscribe and receive updates
[SpacetimeDB.Table(Public = true)]
public partial struct Player { /* ... */ }

// Private table - only visible to module and owner
// Better for internal state, caches, or sensitive data
[SpacetimeDB.Table]
public partial struct InternalState { /* ... */ }
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Public table - clients can subscribe and receive updates
#[spacetimedb::table(name = player, public)]
pub struct Player { /* ... */ }

// Private table - only visible to module and owner
// Better for internal state, caches, or sensitive data
#[spacetimedb::table(name = internal_state)]
pub struct InternalState { /* ... */ }
```

</TabItem>
</Tabs>

Make tables public only when clients need to access them. Private tables:
- Don't consume client bandwidth
- Don't require client-side storage
- Are hidden from non-owner queries

## Batch Operations

When inserting or updating multiple rows, batch them in a single reducer call rather than making multiple reducer calls:

✅ **Good - Batch operation:**

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.reducer('spawn_enemies', { count: t.u32() }, (ctx, { count }) => {
  for (let i = 0; i < count; i++) {
    ctx.db.enemy.insert({
      id: 0, // auto_inc
      health: 100,
    });
  }
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer]
public static void SpawnEnemies(ReducerContext ctx, uint count)
{
    for (uint i = 0; i < count; i++)
    {
        ctx.Db.Enemy.Insert(new Enemy
        {
            Id = 0, // auto_inc
            Health = 100
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::reducer]
fn spawn_enemies(ctx: &ReducerContext, count: u32) {
    for i in 0..count {
        ctx.db.enemy().insert(Enemy {
            id: 0, // auto_inc
            health: 100,
        });
    }
}
```

</TabItem>
</Tabs>

❌ **Avoid - Multiple calls:**

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Client makes 10 separate reducer calls
for (let i = 0; i < 10; i++) {
  connection.reducers.spawnEnemy();
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Client makes 10 separate reducer calls
for (int i = 0; i < 10; i++)
{
    connection.Reducers.SpawnEnemy();
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Client makes 10 separate reducer calls
for i in 0..10 {
    connection.reducers.spawn_enemy();
}
```

</TabItem>
</Tabs>

Batch operations are more efficient because:
- Single transaction reduces overhead
- Reduced network round trips
- Better database performance

## Monitor Table Growth

Be mindful of unbounded table growth:

- Implement cleanup reducers for temporary data
- Archive or delete old records
- Use schedule tables to automatically expire data
- Consider pagination for large result sets

## Next Steps

- Learn about [Indexes](/tables/indexes) to optimize queries
- Explore [Subscriptions](/subscriptions) for efficient client data sync
- Review [Reducers](/functions/reducers) for efficient data modification patterns
