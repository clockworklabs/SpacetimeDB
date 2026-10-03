---
title: How to make a top 100 leaderboard
slug: /guides/games/top-100-leaderboard
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

In this guide, you'll build a leaderboard that shows the 100 best scores in your game and updates live on every client. You'll use an [index](../../00200-core-concepts/00300-tables/00300-indexes.md) to read scores, an anonymous [view](../../00200-core-concepts/00200-functions/00500-views.md) to pick the top 100, and a [subscription](../../00200-core-concepts/00400-subscriptions.md) to keep each client up to date.

Before starting, make sure you have a module and a client connected to it. If you don't, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.

Here's how the pieces fit together:

- A private `player_score` table stores each player's best score, with an index on the score.
- A `submit_score` reducer records a new score.
- A public `top_100` view reads every score through the index and returns the 100 highest.
- Clients subscribe to `top_100`, never to `player_score` itself.

The view is anonymous: it returns the same rows no matter who asks. SpacetimeDB computes it once and shares the result with every subscribed client, instead of computing it once per client.

## Define the table

Add a table that holds one row per player, with an index on the `score` column. The view needs this index: views can't scan a whole table with `iter()`, so they read rows through indexes instead.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, Range } from 'spacetimedb/server';

const playerScore = table(
  { name: 'player_score' },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    score: t.u64().index('btree'),
  }
);

const spacetimedb = schema({ playerScore });
export default spacetimedb;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "PlayerScore")]
    public partial struct PlayerScore
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;

        public string Name;

        [SpacetimeDB.Index.BTree]
        public ulong Score;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{view, AnonymousViewContext, Identity, ReducerContext, Table};

#[spacetimedb::table(accessor = player_score)]
pub struct PlayerScore {
    #[primary_key]
    identity: Identity,
    name: String,
    #[index(btree)]
    score: u64,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
#include <algorithm>
using namespace SpacetimeDB;

struct PlayerScore {
    Identity identity;
    std::string name;
    uint64_t score;
};
SPACETIMEDB_STRUCT(PlayerScore, identity, name, score)
SPACETIMEDB_TABLE(PlayerScore, player_score, Private)
FIELD_PrimaryKey(player_score, identity)
FIELD_Index(player_score, score)
```

</TabItem>
</Tabs>

The table is private, so clients can't subscribe to it directly. They only see the 100 rows the view returns, however many players you have.

:::tip
Keep scores in their own table. The view re-runs every time a row in `player_score` changes. If the score lived on a row that changes often, like a player's position, the leaderboard would be recomputed on every movement.
:::

## Submit a score

Add a reducer that records a score for the player who calls it. It only keeps a player's best score: a lower score than the one already stored is ignored.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const submitScore = spacetimedb.reducer(
  { name: t.string(), score: t.u64() },
  (ctx, { name, score }) => {
    const existing = ctx.db.playerScore.identity.find(ctx.sender);
    if (!existing) {
      ctx.db.playerScore.insert({ identity: ctx.sender, name, score });
    } else if (score > existing.score) {
      // Keep the player's best score.
      ctx.db.playerScore.identity.update({ ...existing, name, score });
    }
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void SubmitScore(ReducerContext ctx, string name, ulong score)
    {
        if (ctx.Db.PlayerScore.Identity.Find(ctx.Sender) is not PlayerScore existing)
        {
            ctx.Db.PlayerScore.Insert(new PlayerScore { Identity = ctx.Sender, Name = name, Score = score });
        }
        else if (score > existing.Score)
        {
            // Keep the player's best score.
            existing.Name = name;
            existing.Score = score;
            ctx.Db.PlayerScore.Identity.Update(existing);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::reducer]
pub fn submit_score(ctx: &ReducerContext, name: String, score: u64) {
    let table = ctx.db.player_score();
    match table.identity().find(ctx.sender()) {
        // Keep the player's best score.
        Some(existing) if existing.score >= score => {}
        Some(existing) => {
            table.identity().update(PlayerScore { name, score, ..existing });
        }
        None => {
            table.insert(PlayerScore {
                identity: ctx.sender(),
                name,
                score,
            });
        }
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(submit_score, ReducerContext ctx, std::string name, uint64_t score) {
    auto existing = ctx.db[player_score_identity].find(ctx.sender());
    if (!existing) {
        ctx.db[player_score].insert(PlayerScore{ctx.sender(), name, score});
    } else if (score > existing->score) {
        // Keep the player's best score.
        PlayerScore updated = *existing;
        updated.name = name;
        updated.score = score;
        ctx.db[player_score_identity].update(updated);
    }
    return Ok();
}
```

</TabItem>
</Tabs>

## Define the top 100 view

Add a public anonymous view named `top_100`. It reads every score with a range query on the index, starting at 0 and with no upper bound. Then it sorts the rows from highest to lowest score and keeps the first 100.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const LEADERBOARD_SIZE = 100;

export const top100 = spacetimedb.anonymousView(
  { name: 'top_100', public: true },
  t.array(playerScore.rowType),
  ctx => {
    const allScores = new Range({ tag: 'included', value: 0n }, { tag: 'unbounded' });
    return Array.from(ctx.db.playerScore.score.filter(allScores))
      .sort((a, b) => (a.score < b.score ? 1 : a.score > b.score ? -1 : 0))
      .slice(0, LEADERBOARD_SIZE);
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int LeaderboardSize = 100;

    [SpacetimeDB.View(Accessor = "Top100", Public = true)]
    public static List<PlayerScore> Top100(AnonymousViewContext ctx)
    {
        return ctx.Db.PlayerScore.Score.Filter((0UL, ulong.MaxValue))
            .OrderByDescending(row => row.Score)
            .Take(LeaderboardSize)
            .ToList();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const LEADERBOARD_SIZE: usize = 100;

#[view(accessor = top_100, public)]
fn top_100(ctx: &AnonymousViewContext) -> Vec<PlayerScore> {
    let mut scores: Vec<PlayerScore> = ctx.db.player_score().score().filter(0u64..).collect();
    scores.sort_by(|a, b| b.score.cmp(&a.score));
    scores.truncate(LEADERBOARD_SIZE);
    scores
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
constexpr size_t LEADERBOARD_SIZE = 100;

SPACETIMEDB_VIEW(std::vector<PlayerScore>, top_100, Public, AnonymousViewContext ctx) {
    auto scores = ctx.db[player_score_score].filter(range_from(uint64_t(0))).collect();
    std::sort(scores.begin(), scores.end(), [](const PlayerScore& a, const PlayerScore& b) {
        return a.score > b.score;
    });
    if (scores.size() > LEADERBOARD_SIZE) {
        scores.erase(scores.begin() + LEADERBOARD_SIZE, scores.end());
    }
    return scores;
}
```

</TabItem>
</Tabs>

:::warning
Always sort inside the view, even though the rows come from an index. When a reducer changes a score, the view re-runs inside that reducer's transaction, and the rows that reducer just wrote can come out of the index before all the others. If you relied on the index order, a new high score could be left off the leaderboard.
:::

### What this view costs

The range query covers every score, so the view reads the whole `player_score` table each time it runs. SpacetimeDB re-runs it whenever a row in `player_score` changes. Because the view is anonymous, that happens once per change, not once per subscribed client.

Because the view reads and sorts the whole table every time a score changes, its cost grows with your player count. That's fine for small and medium games. If you expect a very large player base, measure how long score submissions take as the table grows. See [Performance Considerations](../../00200-core-concepts/00200-functions/00500-views.md#performance-considerations) for more on what makes a view expensive.

## Subscribe from the client

On the client, subscribe to `top_100` and read the rows from the client cache. The client cache doesn't keep the order the view returned its rows in, so sort them again before you display them.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx
      .subscriptionBuilder()
      .onApplied(() => renderLeaderboard())
      .subscribe([tables.top100]);
  })
  .build();

// The client cache doesn't keep the view's order, so sort the rows here.
function getLeaderboard() {
  return Array.from(conn.db.top100.iter()).sort((a, b) =>
    a.score < b.score ? 1 : a.score > b.score ? -1 : 0
  );
}

function renderLeaderboard() {
  getLeaderboard().forEach((row, i) => {
    console.log(`${i + 1}. ${row.name} - ${row.score}`);
  });
}

// Re-render when a score enters or leaves the top 100.
conn.db.top100.onInsert(() => renderLeaderboard());
conn.db.top100.onDelete(() => renderLeaderboard());

// Call this when a game ends to submit the player's score.
function onGameOver(name: string, score: bigint) {
  conn.reducers.submitScore({ name, score });
}
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
            .OnApplied(ctx => RenderLeaderboard(ctx.Db))
            .AddQuery(q => q.From.Top100())
            .Subscribe();
    })
    .Build();

// Re-render when a score enters or leaves the top 100.
conn.Db.Top100.OnInsert += (ctx, row) => RenderLeaderboard(ctx.Db);
conn.Db.Top100.OnDelete += (ctx, row) => RenderLeaderboard(ctx.Db);

// Call this when a game ends to submit the player's score.
void OnGameOver(string name, ulong score) => conn.Reducers.SubmitScore(name, score);

// The client cache doesn't keep the view's order, so sort the rows here.
static List<PlayerScore> GetLeaderboard(RemoteTables db) =>
    db.Top100.Iter().OrderByDescending(row => row.Score).ToList();

static void RenderLeaderboard(RemoteTables db)
{
    var rows = GetLeaderboard(db);
    for (var i = 0; i < rows.Count; i++)
    {
        Console.WriteLine($"{i + 1}. {rows[i].Name} - {rows[i].Score}");
    }
}
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
                .on_applied(|ctx| render_leaderboard(&ctx.db))
                .add_query(|q| q.from.top_100())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // Re-render when a score enters or leaves the top 100.
    conn.db().top_100().on_insert(|ctx, _row| render_leaderboard(&ctx.db));
    conn.db().top_100().on_delete(|ctx, _row| render_leaderboard(&ctx.db));

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

// Call this when a game ends to submit the player's score.
fn on_game_over(conn: &DbConnection, name: String, score: u64) {
    conn.reducers().submit_score(name, score).unwrap();
}

// The client cache doesn't keep the view's order, so sort the rows here.
fn leaderboard(db: &RemoteTables) -> Vec<PlayerScore> {
    let mut rows: Vec<PlayerScore> = db.top_100().iter().collect();
    rows.sort_by(|a, b| b.score.cmp(&a.score));
    rows
}

fn render_leaderboard(db: &RemoteTables) {
    for (i, row) in leaderboard(db).iter().enumerate() {
        println!("{}. {} - {}", i + 1, row.name, row.score);
    }
}
```

</TabItem>
</Tabs>

`onInsert` and `onDelete` run once for each row that changes. When a new score enters the top 100, the client receives one insert for the new row and one delete for the score that dropped out, so the leaderboard is re-rendered twice. If rendering is expensive, re-render at most once per frame instead.

## What's next?

You now have a leaderboard that stays live on every client:

- An index on `score` lets the view read the scores.
- An anonymous view sorts them and keeps the top 100, computed once for all clients.
- Clients subscribe to the view and sort the rows again before displaying them.

To learn more about the features used here, see [Indexes](../../00200-core-concepts/00300-tables/00300-indexes.md), [Views](../../00200-core-concepts/00200-functions/00500-views.md) and [Subscriptions](../../00200-core-concepts/00400-subscriptions.md).

Next, you can [reset the leaderboard each season](./00700-seasonal-leaderboard-reset.md).
