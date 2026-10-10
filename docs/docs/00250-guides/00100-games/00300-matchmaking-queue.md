---
title: Build a matchmaking queue
slug: /guides/games/matchmaking-queue
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll build a matchmaking queue: players ask to find a match, wait in a queue, and are put into a 4-player match as soon as enough players are waiting. You'll use [reducers](../../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md) to join and leave the queue, the `client_disconnected` [lifecycle reducer](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md) to clean up after players who leave, and a per-player [view](../../00200-core-concepts/00200-functions/00500-views.md) to tell each player about their match.

## How it works

- A private `queue_entry` table holds the players who are waiting.
- A `join_queue` reducer adds the caller to the queue, then creates a match if enough players are waiting.
- A `leave_queue` reducer and the `client_disconnected` reducer remove players from the queue.
- Private `game_match` and `match_player` tables record each match and its players.
- A public `my_match` view returns the players in the caller's match, and nothing else.

## Define the tables

Add three tables. All of them are private: players only learn about their own match, through the view.

- `queue_entry` has one row per waiting player, with the time they joined.
- `game_match` has one row per match.
- `match_player` has one row per player in a match. Its primary key is the player's identity, so a player can only be in one match at a time. It has an index on `match_id`, to find every player in a match.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, SenderError, type InferSchema, type ReducerCtx } from 'spacetimedb/server';

// Number of players in each match.
const MATCH_SIZE = 4;

const queueEntry = table(
  { name: 'queue_entry' },
  {
    identity: t.identity().primaryKey(),
    joinedAt: t.timestamp(),
  }
);

const gameMatch = table(
  { name: 'game_match' },
  {
    id: t.u64().primaryKey().autoInc(),
    createdAt: t.timestamp(),
  }
);

const matchPlayer = table(
  { name: 'match_player' },
  {
    identity: t.identity().primaryKey(),
    matchId: t.u64().index('btree'),
  }
);

const spacetimedb = schema({ queueEntry, gameMatch, matchPlayer });
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    // Number of players in each match.
    const int MatchSize = 4;

    [SpacetimeDB.Table(Accessor = "QueueEntry")]
    public partial struct QueueEntry
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public Timestamp JoinedAt;
    }

    [SpacetimeDB.Table(Accessor = "GameMatch")]
    public partial struct GameMatch
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public Timestamp CreatedAt;
    }

    [SpacetimeDB.Table(Accessor = "MatchPlayer")]
    public partial struct MatchPlayer
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;

        [SpacetimeDB.Index.BTree]
        public ulong MatchId;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, view, Identity, ReducerContext, Table, Timestamp, ViewContext};

// Number of players in each match.
const MATCH_SIZE: usize = 4;

#[table(accessor = queue_entry)]
pub struct QueueEntry {
    #[primary_key]
    identity: Identity,
    joined_at: Timestamp,
}

#[table(accessor = game_match)]
pub struct GameMatch {
    #[primary_key]
    #[auto_inc]
    id: u64,
    created_at: Timestamp,
}

#[table(accessor = match_player)]
pub struct MatchPlayer {
    #[primary_key]
    identity: Identity,
    #[index(btree)]
    match_id: u64,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
#include <algorithm>
using namespace SpacetimeDB;

// Number of players in each match.
constexpr size_t MATCH_SIZE = 4;

struct QueueEntry {
    Identity identity;
    Timestamp joined_at;
};
SPACETIMEDB_STRUCT(QueueEntry, identity, joined_at)
SPACETIMEDB_TABLE(QueueEntry, queue_entry, Private)
FIELD_PrimaryKey(queue_entry, identity)

struct GameMatch {
    uint64_t id;
    Timestamp created_at;
};
SPACETIMEDB_STRUCT(GameMatch, id, created_at)
SPACETIMEDB_TABLE(GameMatch, game_match, Private)
FIELD_PrimaryKeyAutoInc(game_match, id)

struct MatchPlayer {
    Identity identity;
    uint64_t match_id;
};
SPACETIMEDB_STRUCT(MatchPlayer, identity, match_id)
SPACETIMEDB_TABLE(MatchPlayer, match_player, Private)
FIELD_PrimaryKey(match_player, identity)
FIELD_Index(match_player, match_id)
```

</TabItem>
</Tabs>

## Join the queue and create matches

Add a `join_queue` reducer. It refuses players who are already waiting or already in a match, adds the caller to the queue, then checks whether enough players are waiting to start a match.

If they are, the reducer creates a match with the players who have waited longest and removes them from the queue. A reducer runs as a single transaction, so two players joining at the same moment can never be put into two matches, or into a match with a player who just left.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const joinQueue = spacetimedb.reducer(ctx => {
  if (ctx.db.queueEntry.identity.find(ctx.sender)) {
    throw new SenderError('already in the queue');
  }
  if (ctx.db.matchPlayer.identity.find(ctx.sender)) {
    throw new SenderError('already in a match');
  }
  ctx.db.queueEntry.insert({ identity: ctx.sender, joinedAt: ctx.timestamp });
  tryCreateMatch(ctx);
});

function tryCreateMatch(ctx: Ctx) {
  const waiting = Array.from(ctx.db.queueEntry.iter());
  if (waiting.length < MATCH_SIZE) {
    return;
  }
  // Match the players who have waited longest.
  waiting.sort((a, b) => {
    const aTime = a.joinedAt.microsSinceUnixEpoch;
    const bTime = b.joinedAt.microsSinceUnixEpoch;
    return aTime < bTime ? -1 : aTime > bTime ? 1 : 0;
  });

  const game = ctx.db.gameMatch.insert({ id: 0n, createdAt: ctx.timestamp });
  for (const entry of waiting.slice(0, MATCH_SIZE)) {
    ctx.db.queueEntry.identity.delete(entry.identity);
    ctx.db.matchPlayer.insert({ identity: entry.identity, matchId: game.id });
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void JoinQueue(ReducerContext ctx)
    {
        if (ctx.Db.QueueEntry.Identity.Find(ctx.Sender) is not null)
        {
            throw new Exception("Already in the queue");
        }
        if (ctx.Db.MatchPlayer.Identity.Find(ctx.Sender) is not null)
        {
            throw new Exception("Already in a match");
        }
        ctx.Db.QueueEntry.Insert(new QueueEntry { Identity = ctx.Sender, JoinedAt = ctx.Timestamp });
        TryCreateMatch(ctx);
    }

    static void TryCreateMatch(ReducerContext ctx)
    {
        var waiting = ctx.Db.QueueEntry.Iter().ToList();
        if (waiting.Count < MatchSize)
        {
            return;
        }

        var game = ctx.Db.GameMatch.Insert(new GameMatch { Id = 0, CreatedAt = ctx.Timestamp });
        // Match the players who have waited longest.
        foreach (var entry in waiting.OrderBy(e => e.JoinedAt.MicrosecondsSinceUnixEpoch).Take(MatchSize))
        {
            ctx.Db.QueueEntry.Identity.Delete(entry.Identity);
            ctx.Db.MatchPlayer.Insert(new MatchPlayer { Identity = entry.Identity, MatchId = game.Id });
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn join_queue(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.db.queue_entry().identity().find(ctx.sender()).is_some() {
        return Err("already in the queue".to_string());
    }
    if ctx.db.match_player().identity().find(ctx.sender()).is_some() {
        return Err("already in a match".to_string());
    }
    ctx.db.queue_entry().insert(QueueEntry {
        identity: ctx.sender(),
        joined_at: ctx.timestamp,
    });
    try_create_match(ctx);
    Ok(())
}

fn try_create_match(ctx: &ReducerContext) {
    let mut waiting: Vec<QueueEntry> = ctx.db.queue_entry().iter().collect();
    if waiting.len() < MATCH_SIZE {
        return;
    }
    // Match the players who have waited longest.
    waiting.sort_by_key(|entry| entry.joined_at);

    let game = ctx.db.game_match().insert(GameMatch {
        id: 0,
        created_at: ctx.timestamp,
    });
    for entry in waiting.into_iter().take(MATCH_SIZE) {
        ctx.db.queue_entry().identity().delete(entry.identity);
        ctx.db.match_player().insert(MatchPlayer {
            identity: entry.identity,
            match_id: game.id,
        });
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
void try_create_match(ReducerContext& ctx) {
    std::vector<QueueEntry> waiting;
    for (const auto& entry : ctx.db[queue_entry]) {
        waiting.push_back(entry);
    }
    if (waiting.size() < MATCH_SIZE) {
        return;
    }
    // Match the players who have waited longest.
    std::sort(waiting.begin(), waiting.end(), [](const QueueEntry& a, const QueueEntry& b) {
        return a.joined_at.micros_since_epoch() < b.joined_at.micros_since_epoch();
    });

    GameMatch game = ctx.db[game_match].insert(GameMatch{0, ctx.timestamp});
    for (size_t i = 0; i < MATCH_SIZE; ++i) {
        ctx.db[queue_entry_identity].delete_by_key(waiting[i].identity);
        ctx.db[match_player].insert(MatchPlayer{waiting[i].identity, game.id});
    }
}

SPACETIMEDB_REDUCER(join_queue, ReducerContext ctx) {
    if (ctx.db[queue_entry_identity].find(ctx.sender())) {
        return Err("already in the queue");
    }
    if (ctx.db[match_player_identity].find(ctx.sender())) {
        return Err("already in a match");
    }
    ctx.db[queue_entry].insert(QueueEntry{ctx.sender(), ctx.timestamp});
    try_create_match(ctx);
    return Ok();
}
```

</TabItem>
</Tabs>

Because a match is created as soon as enough players are waiting, the queue never holds more than 3 players at a time, so reading the whole queue in `join_queue` stays cheap.

## Leave the queue

Players should be able to cancel their search. Add a `leave_queue` reducer, and remove players from the queue when they disconnect, so nobody is matched with a player who already closed the game.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const leaveQueue = spacetimedb.reducer(ctx => {
  ctx.db.queueEntry.identity.delete(ctx.sender);
});

export const onDisconnect = spacetimedb.clientDisconnected(ctx => {
  ctx.db.queueEntry.identity.delete(ctx.sender);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void LeaveQueue(ReducerContext ctx)
    {
        ctx.Db.QueueEntry.Identity.Delete(ctx.Sender);
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        ctx.Db.QueueEntry.Identity.Delete(ctx.Sender);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn leave_queue(ctx: &ReducerContext) {
    ctx.db.queue_entry().identity().delete(ctx.sender());
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    ctx.db.queue_entry().identity().delete(ctx.sender());
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(leave_queue, ReducerContext ctx) {
    ctx.db[queue_entry_identity].delete_by_key(ctx.sender());
    return Ok();
}

SPACETIMEDB_CLIENT_DISCONNECTED(client_disconnected, ReducerContext ctx) {
    ctx.db[queue_entry_identity].delete_by_key(ctx.sender());
    return Ok();
}
```

</TabItem>
</Tabs>

## Tell players about their match

Add a public `my_match` view. It looks up the caller's row in `match_player`, then returns every player in the same match. Players who aren't in a match get an empty result.

Unlike an anonymous view, this view depends on who is asking, so SpacetimeDB computes it separately for each subscribed player. That's what keeps one player from seeing another player's match.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const myMatch = spacetimedb.view(
  { name: 'my_match', public: true },
  t.array(matchPlayer.rowType),
  ctx => {
    const me = ctx.db.matchPlayer.identity.find(ctx.sender);
    if (!me) {
      return [];
    }
    return Array.from(ctx.db.matchPlayer.matchId.filter(me.matchId));
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.View(Accessor = "MyMatch", Public = true)]
    public static List<MatchPlayer> MyMatch(ViewContext ctx)
    {
        if (ctx.Db.MatchPlayer.Identity.Find(ctx.Sender) is not MatchPlayer me)
        {
            return new List<MatchPlayer>();
        }
        return ctx.Db.MatchPlayer.MatchId.Filter(me.MatchId).ToList();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[view(accessor = my_match, public)]
fn my_match(ctx: &ViewContext) -> Vec<MatchPlayer> {
    let Some(me) = ctx.db.match_player().identity().find(ctx.sender()) else {
        return vec![];
    };
    ctx.db.match_player().match_id().filter(me.match_id).collect()
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_VIEW(std::vector<MatchPlayer>, my_match, Public, ViewContext ctx) {
    auto me = ctx.db[match_player_identity].find(ctx.sender());
    if (!me) {
        return {};
    }
    return ctx.db[match_player_match_id].filter(me->match_id).collect();
}
```

</TabItem>
</Tabs>

When a match ends, delete its `match_player` rows so its players can join the queue again.

## Find a match from the client

On the client, subscribe to `my_match`, and call `join_queue` when the player asks to find a match. When the match is created, the client receives one row for each player in it, including the player's own row. Use the player's own row as the signal that a match was found.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.myMatch]);
  })
  .build();

// Call this when the player presses "Find match".
function findMatch() {
  conn.reducers.joinQueue({});
}

// Call this when the player cancels the search.
function cancelSearch() {
  conn.reducers.leaveQueue({});
}

// The player's own row arrives once, when their match is created.
conn.db.myMatch.onInsert((_ctx, player) => {
  if (!player.identity.isEqual(conn.identity!)) {
    return;
  }
  const players = Array.from(conn.db.myMatch.iter());
  console.log(`Match ${player.matchId} found with ${players.length} players`);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.MyMatch())
            .Subscribe();
    })
    .Build();

// Call this when the player presses "Find match".
void FindMatch() => conn.Reducers.JoinQueue();

// Call this when the player cancels the search.
void CancelSearch() => conn.Reducers.LeaveQueue();

// The player's own row arrives once, when their match is created.
conn.Db.MyMatch.OnInsert += (ctx, player) =>
{
    if (player.Identity != conn.Identity)
    {
        return;
    }
    var players = ctx.Db.MyMatch.Iter().ToList();
    Console.WriteLine($"Match {player.MatchId} found with {players.Count} players");
};
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Table};

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .add_query(|q| q.from.my_match())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // The player's own row arrives once, when their match is created.
    conn.db().my_match().on_insert(|ctx, player| {
        if Some(player.identity) != ctx.try_identity() {
            return;
        }
        let players = ctx.db.my_match().count();
        println!("Match {} found with {} players", player.match_id, players);
    });

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

// Call this when the player presses "Find match".
fn find_match(conn: &DbConnection) {
    conn.reducers().join_queue().unwrap();
}

// Call this when the player cancels the search.
fn cancel_search(conn: &DbConnection) {
    conn.reducers().leave_queue().unwrap();
}
```

</TabItem>
</Tabs>

## What's next?

You now have a queue that groups waiting players into 4-player matches:

- `join_queue` adds players to the queue and creates a match as soon as enough of them are waiting, all in one transaction.
- `leave_queue` and `client_disconnected` keep players who left out of new matches.
- The per-player `my_match` view tells each player about their own match, and only theirs.

To learn more about the features used here, see [Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md), [Lifecycle Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md) and [Views](../../00200-core-concepts/00200-functions/00500-views.md).

Next, you can [make a turn-based game with turn timeouts](./00400-turn-based-game.md).
