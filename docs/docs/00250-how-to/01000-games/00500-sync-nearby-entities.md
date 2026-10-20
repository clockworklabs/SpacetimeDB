---
title: "Only sync what's near the player"
slug: /how-to/games/sync-nearby-entities
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll make each client receive only the players near it, rather than every player in the world. You'll split the world into square chunks, store the chunk each player is in, and have each client [subscribe](../../00200-core-concepts/00400-subscriptions.md) to the chunks around its player, moving that subscription as the player moves.

:::warning This doesn't hide players
Clients choose what they subscribe to, and the `player` table is public, so a modified client can subscribe to every chunk and see every player. This guide reduces what honest clients download. If players must not be able to see where other players are, for example hidden units in a strategy game, the server has to decide what each client can see: see [Hide information from players](./00600-hide-information.md).
:::

## How it works

- The world is a grid of square chunks, each 100 units wide.
- A public `player` table holds each player's position and the chunk they're in, with an index on the chunk.
- A `move_player` reducer updates the player's position and chunk.
- Each client subscribes to its own row, and to the players in its chunk and the 8 chunks around it.
- When the player enters a new chunk, the client subscribes to the new chunks, then drops its old subscription.

## Define the table

Add a `player` table with one row per online player. `chunk` is a single number that identifies a chunk: `chunk_y * 100 + chunk_x`. Its index lets SpacetimeDB find the players in a chunk directly, without scanning the whole table.

The world here is 10,000 units wide, or 100 chunks per side. Use the size of your own world.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, SenderError } from 'spacetimedb/server';

// The world is a square, split into 100 x 100 square chunks.
const WORLD_SIZE = 10_000;
const CHUNK_SIZE = 100;
const CHUNKS_PER_SIDE = 100;

const player = table(
  { name: 'player', public: true },
  {
    identity: t.identity().primaryKey(),
    x: t.f32(),
    y: t.f32(),
    // The chunk the player is in. Updated whenever the player moves.
    chunk: t.u32().index('btree'),
  }
);

const spacetimedb = schema({ player });
export default spacetimedb;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    // The world is a square, split into 100 x 100 square chunks.
    const float WorldSize = 10_000f;
    const float ChunkSize = 100f;
    const uint ChunksPerSide = 100;

    [SpacetimeDB.Table(Accessor = "Player", Public = true)]
    public partial struct Player
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public float X;
        public float Y;

        // The chunk the player is in. Updated whenever the player moves.
        [SpacetimeDB.Index.BTree]
        public uint Chunk;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

// The world is a square, split into 100 x 100 square chunks.
const WORLD_SIZE: f32 = 10_000.0;
const CHUNK_SIZE: f32 = 100.0;
const CHUNKS_PER_SIDE: u32 = 100;

#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    x: f32,
    y: f32,
    // The chunk the player is in. Updated whenever the player moves.
    #[index(btree)]
    chunk: u32,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

// The world is a square, split into 100 x 100 square chunks.
constexpr float WORLD_SIZE = 10'000.0f;
constexpr float CHUNK_SIZE = 100.0f;
constexpr uint32_t CHUNKS_PER_SIDE = 100;

struct Player {
    Identity identity;
    float x;
    float y;
    // The chunk the player is in. Updated whenever the player moves.
    uint32_t chunk;
};
SPACETIMEDB_STRUCT(Player, identity, x, y, chunk)
SPACETIMEDB_TABLE(Player, player, Public)
FIELD_PrimaryKey(player, identity)
FIELD_Index(player, chunk)
```

</TabItem>
</Tabs>

## Track each player's chunk

Add a `chunk_at` function that returns the chunk containing a position, and keep `chunk` up to date wherever a player's position changes:

- The `client_connected` [lifecycle reducer](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md) adds the player in the middle of the world. If the player already has a row, for example because they're connected from two tabs, it keeps it.
- The `client_disconnected` reducer removes the player, so clients only see players who are online.
- The `move_player` reducer rejects positions outside the world, then stores the new position and its chunk.

`move_player` accepts any position in the world. In your game, also check that the player didn't move further than they can.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Returns the chunk that contains the position (x, y).
function chunkAt(x: number, y: number): number {
  const chunkX = Math.floor(x / CHUNK_SIZE);
  const chunkY = Math.floor(y / CHUNK_SIZE);
  return chunkY * CHUNKS_PER_SIDE + chunkX;
}

export const onConnect = spacetimedb.clientConnected(ctx => {
  if (ctx.db.player.identity.find(ctx.sender)) {
    return;
  }
  // New players appear in the middle of the world.
  const x = WORLD_SIZE / 2;
  const y = WORLD_SIZE / 2;
  ctx.db.player.insert({ identity: ctx.sender, x, y, chunk: chunkAt(x, y) });
});

export const onDisconnect = spacetimedb.clientDisconnected(ctx => {
  ctx.db.player.identity.delete(ctx.sender);
});

export const movePlayer = spacetimedb.reducer({ x: t.f32(), y: t.f32() }, (ctx, { x, y }) => {
  if (!(x >= 0 && x < WORLD_SIZE && y >= 0 && y < WORLD_SIZE)) {
    throw new SenderError('position is outside the world');
  }
  const row = ctx.db.player.identity.find(ctx.sender);
  if (!row) {
    throw new SenderError('player not found');
  }
  ctx.db.player.identity.update({ ...row, x, y, chunk: chunkAt(x, y) });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Returns the chunk that contains the position (x, y).
    static uint ChunkAt(float x, float y)
    {
        var chunkX = (uint)(x / ChunkSize);
        var chunkY = (uint)(y / ChunkSize);
        return chunkY * ChunksPerSide + chunkX;
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        if (ctx.Db.Player.Identity.Find(ctx.Sender) is not null)
        {
            return;
        }
        // New players appear in the middle of the world.
        float x = WorldSize / 2, y = WorldSize / 2;
        ctx.Db.Player.Insert(new Player { Identity = ctx.Sender, X = x, Y = y, Chunk = ChunkAt(x, y) });
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        ctx.Db.Player.Identity.Delete(ctx.Sender);
    }

    [SpacetimeDB.Reducer]
    public static void MovePlayer(ReducerContext ctx, float x, float y)
    {
        if (!(x >= 0 && x < WorldSize && y >= 0 && y < WorldSize))
        {
            throw new Exception("Position is outside the world");
        }
        var player = ctx.Db.Player.Identity.Find(ctx.Sender) ?? throw new Exception("Player not found");
        player.X = x;
        player.Y = y;
        player.Chunk = ChunkAt(x, y);
        ctx.Db.Player.Identity.Update(player);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Returns the chunk that contains the position (x, y).
fn chunk_at(x: f32, y: f32) -> u32 {
    let chunk_x = (x / CHUNK_SIZE) as u32;
    let chunk_y = (y / CHUNK_SIZE) as u32;
    chunk_y * CHUNKS_PER_SIDE + chunk_x
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    if ctx.db.player().identity().find(ctx.sender()).is_some() {
        return;
    }
    // New players appear in the middle of the world.
    let (x, y) = (WORLD_SIZE / 2.0, WORLD_SIZE / 2.0);
    ctx.db.player().insert(Player {
        identity: ctx.sender(),
        x,
        y,
        chunk: chunk_at(x, y),
    });
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    ctx.db.player().identity().delete(ctx.sender());
}

#[reducer]
pub fn move_player(ctx: &ReducerContext, x: f32, y: f32) -> Result<(), String> {
    if !(0.0..WORLD_SIZE).contains(&x) || !(0.0..WORLD_SIZE).contains(&y) {
        return Err("position is outside the world".to_string());
    }
    let mut player = ctx.db.player().identity().find(ctx.sender()).ok_or("player not found")?;
    player.x = x;
    player.y = y;
    player.chunk = chunk_at(x, y);
    ctx.db.player().identity().update(player);
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// Returns the chunk that contains the position (x, y).
uint32_t chunk_at(float x, float y) {
    uint32_t chunk_x = static_cast<uint32_t>(x / CHUNK_SIZE);
    uint32_t chunk_y = static_cast<uint32_t>(y / CHUNK_SIZE);
    return chunk_y * CHUNKS_PER_SIDE + chunk_x;
}

SPACETIMEDB_CLIENT_CONNECTED(client_connected, ReducerContext ctx) {
    if (ctx.db[player_identity].find(ctx.sender())) {
        return Ok();
    }
    // New players appear in the middle of the world.
    float x = WORLD_SIZE / 2, y = WORLD_SIZE / 2;
    ctx.db[player].insert(Player{ctx.sender(), x, y, chunk_at(x, y)});
    return Ok();
}

SPACETIMEDB_CLIENT_DISCONNECTED(client_disconnected, ReducerContext ctx) {
    ctx.db[player_identity].delete_by_key(ctx.sender());
    return Ok();
}

SPACETIMEDB_REDUCER(move_player, ReducerContext ctx, float x, float y) {
    if (!(x >= 0 && x < WORLD_SIZE && y >= 0 && y < WORLD_SIZE)) {
        return Err("position is outside the world");
    }
    auto found = ctx.db[player_identity].find(ctx.sender());
    if (!found) {
        return Err("player not found");
    }
    Player updated = *found;
    updated.x = x;
    updated.y = y;
    updated.chunk = chunk_at(x, y);
    ctx.db[player_identity].update(updated);
    return Ok();
}
```

</TabItem>
</Tabs>

## Subscribe to nearby players

The client keeps two subscriptions:

- **Its own row**, for its whole session. That's how it learns which chunk the player is in.
- **The nearby chunks**: one query per chunk, for the player's chunk and the chunks around it. Each query keeps only the rows whose `chunk` equals one value, which is the cheapest kind of query to keep up to date. When a player moves, SpacetimeDB only checks the subscriptions to that player's old and new chunks, however many clients are subscribed to other chunks.

When the player's own row shows they entered a new chunk, the client subscribes to the chunks around it. It drops the old subscription only once the new one is applied. Players in both areas then stay in the client cache, rather than being removed and added back.

The client cache now holds only the nearby players. Draw them from it each frame. When a player leaves the area, the client receives a delete for their row, so remove them from the screen in your delete callback.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables, type SubscriptionHandle } from './module_bindings';

// Must match the module.
const CHUNKS_PER_SIDE = 100;

// Returns the chunk and the chunks around it, up to 9 of them.
function chunksAround(chunk: number): number[] {
  const chunkX = chunk % CHUNKS_PER_SIDE;
  const chunkY = Math.floor(chunk / CHUNKS_PER_SIDE);
  const chunks = [];
  for (let y = chunkY - 1; y <= chunkY + 1; y++) {
    for (let x = chunkX - 1; x <= chunkX + 1; x++) {
      if (x >= 0 && x < CHUNKS_PER_SIDE && y >= 0 && y < CHUNKS_PER_SIDE) {
        chunks.push(y * CHUNKS_PER_SIDE + x);
      }
    }
  }
  return chunks;
}

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect((ctx, identity) => {
    // The player's own row tells us which chunk they're in.
    ctx.subscriptionBuilder().subscribe([tables.player.where(p => p.identity.eq(identity))]);
  })
  .build();

let nearby: SubscriptionHandle | undefined;
let nearbyChunk: number | undefined;

function subscribeAround(chunk: number) {
  if (chunk === nearbyChunk) {
    return;
  }
  nearbyChunk = chunk;
  // Subscribe to the new chunks first, and only then drop the old ones,
  // so players in both stay in the client cache.
  const previous = nearby;
  nearby = conn
    .subscriptionBuilder()
    .onApplied(() => previous?.unsubscribe())
    .subscribe(chunksAround(chunk).map(c => tables.player.where(p => p.chunk.eq(c))));
}

// Move the subscription when the player's own row arrives, or changes chunk.
conn.db.player.onInsert((_ctx, player) => {
  if (player.identity.isEqual(conn.identity!)) {
    subscribeAround(player.chunk);
  }
});
conn.db.player.onUpdate((_ctx, _old, player) => {
  if (player.identity.isEqual(conn.identity!)) {
    subscribeAround(player.chunk);
  }
});

// Call this when the player moves.
function move(x: number, y: number) {
  conn.reducers.movePlayer({ x, y });
}

// Each frame, draw the nearby players:
// for (const player of conn.db.player.iter()) { ... }
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

// Must match the module.
const uint ChunksPerSide = 100;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        // The player's own row tells us which chunk they're in.
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.Player().Where(p => p.Identity.Eq(identity)))
            .Subscribe();
    })
    .Build();

SubscriptionHandle? nearby = null;
uint? nearbyChunk = null;

void SubscribeAround(uint chunk)
{
    if (chunk == nearbyChunk)
    {
        return;
    }
    nearbyChunk = chunk;
    // Subscribe to the new chunks first, and only then drop the old ones,
    // so players in both stay in the client cache.
    var previous = nearby;
    var chunks = ChunksAround(chunk);
    var builder = conn.SubscriptionBuilder()
        .OnApplied(ctx => previous?.Unsubscribe())
        .AddQuery(q => q.From.Player().Where(p => p.Chunk.Eq(chunks[0])));
    foreach (var c in chunks.Skip(1))
    {
        builder = builder.AddQuery(q => q.From.Player().Where(p => p.Chunk.Eq(c)));
    }
    nearby = builder.Subscribe();
}

// Move the subscription when the player's own row arrives, or changes chunk.
conn.Db.Player.OnInsert += (ctx, player) =>
{
    if (player.Identity == conn.Identity)
    {
        SubscribeAround(player.Chunk);
    }
};
conn.Db.Player.OnUpdate += (ctx, oldPlayer, player) =>
{
    if (player.Identity == conn.Identity)
    {
        SubscribeAround(player.Chunk);
    }
};

// Call this when the player moves.
void Move(float x, float y) => conn.Reducers.MovePlayer(x, y);

// Each frame, draw the nearby players:
// foreach (var player in conn.Db.Player.Iter()) { ... }

// Returns the chunk and the chunks around it, up to 9 of them.
static List<uint> ChunksAround(uint chunk)
{
    var chunkX = (int)(chunk % ChunksPerSide);
    var chunkY = (int)(chunk / ChunksPerSide);
    var chunks = new List<uint>();
    for (var y = chunkY - 1; y <= chunkY + 1; y++)
    {
        for (var x = chunkX - 1; x <= chunkX + 1; x++)
        {
            if (x >= 0 && x < ChunksPerSide && y >= 0 && y < ChunksPerSide)
            {
                chunks.Add((uint)(y * ChunksPerSide + x));
            }
        }
    }
    return chunks;
}
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, SubscriptionHandle as _, Table, TableWithPrimaryKey};
use std::sync::Mutex;

// Must match the module.
const CHUNKS_PER_SIDE: u32 = 100;

// The chunk the nearby subscription is centered on, and the subscription itself.
static NEARBY: Mutex<Option<(u32, SubscriptionHandle)>> = Mutex::new(None);

// Returns the chunk and the chunks around it, up to 9 of them.
fn chunks_around(chunk: u32) -> Vec<u32> {
    let side = CHUNKS_PER_SIDE as i64;
    let (chunk_x, chunk_y) = (chunk as i64 % side, chunk as i64 / side);
    let mut chunks = Vec::new();
    for y in chunk_y - 1..=chunk_y + 1 {
        for x in chunk_x - 1..=chunk_x + 1 {
            if (0..side).contains(&x) && (0..side).contains(&y) {
                chunks.push((y * side + x) as u32);
            }
        }
    }
    chunks
}

fn subscribe_around(ctx: &EventContext, chunk: u32) {
    let mut nearby = NEARBY.lock().unwrap();
    if matches!(*nearby, Some((current, _)) if current == chunk) {
        return;
    }
    // Subscribe to the new chunks first, and only then drop the old ones,
    // so players in both stay in the client cache.
    let previous = nearby.take().map(|(_, handle)| handle);
    let chunks = chunks_around(chunk);
    let first = chunks[0];
    let mut builder = ctx
        .subscription_builder()
        .on_applied(move |_| {
            if let Some(previous) = previous {
                let _ = previous.unsubscribe();
            }
        })
        .add_query(move |q| q.from.player().r#where(|p| p.chunk.eq(first)));
    for &c in &chunks[1..] {
        builder = builder.add_query(move |q| q.from.player().r#where(|p| p.chunk.eq(c)));
    }
    *nearby = Some((chunk, builder.subscribe()));
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, identity, _token| {
            // The player's own row tells us which chunk they're in.
            ctx.subscription_builder()
                .add_query(move |q| q.from.player().r#where(|p| p.identity.eq(identity)))
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // Move the subscription when the player's own row arrives, or changes chunk.
    conn.db().player().on_insert(|ctx, player| {
        if player.identity == ctx.identity() {
            subscribe_around(ctx, player.chunk);
        }
    });
    conn.db().player().on_update(|ctx, _old, player| {
        if player.identity == ctx.identity() {
            subscribe_around(ctx, player.chunk);
        }
    });

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here. Each frame, draw the nearby players:
    // for player in conn.db().player().iter() { ... }
}

// Call this when the player moves.
fn move_to(conn: &DbConnection, x: f32, y: f32) {
    conn.reducers().move_player(x, y).unwrap();
}
```

</TabItem>
</Tabs>

## What's next?

Each client now receives only the players around it:

- Every player row stores its chunk, in an indexed column that `move_player` keeps up to date.
- Clients subscribe to the 9 chunks around their player, with one query per chunk.
- When the player changes chunk, the client subscribes to the new chunks before dropping the old ones.

To adapt it to your game:

- **Pick the chunk size from your view distance.** The 3×3 area only covers the view distance in every direction if chunks are at least as wide as it. Smaller chunks send fewer players, but clients change subscriptions more often.
- **Sync other things the same way.** Give NPCs, items or projectiles a `chunk` column with an index, and add a query per chunk for their tables to the same subscription.
- **Update chunks in your game tick.** If a [fixed-rate game tick](./00200-fixed-rate-game-tick.md) moves your entities, recompute their chunk there too.

To learn more about the features used here, see [Subscriptions](../../00200-core-concepts/00400-subscriptions.md), [Indexes](../../00200-core-concepts/00300-tables/00300-indexes.md) and [Lifecycle Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md).

Next, you can [hide information from players](./00600-hide-information.md).
