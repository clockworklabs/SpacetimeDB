---
title: Reset a leaderboard each season
slug: /how-to/games/seasonal-leaderboard-reset
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll split a top 100 leaderboard into seasons that last 4 weeks each. When a season ends, its 3 best players go into a hall of fame, and the leaderboard starts over. You'll use a [schedule table](../../00200-core-concepts/00300-tables/00500-schedule-tables.md) to end each season on time, and give each score the season it was set in, so that ending a season doesn't delete anything.

## How it works

- A private `player_score` table stores each player's best score, and the season it was set in.
- A private `season` table holds the current season's number and when it ends.
- When the season ends, a schedule table calls the `end_season` reducer. It records the season's 3 best players in a `season_winner` table, then starts the next season.
- The `top_100` view only reads the current season's scores, so the leaderboard starts empty each season.
- Clients subscribe to three anonymous views: `top_100`, `current_season` and `hall_of_fame`.

Ending a season never deletes a score. Deleting every score at the end of a season would touch every player's row in a single transaction, which takes longer the more players you have. Moving to the next season only changes one row, however many players you have. A player's score from an earlier season is replaced the next time they submit a score.

## Define the tables

Add four tables, all private:

- `player_score` has one row per player: their best score, and the season they set it in. It has an index on `season`: views can't scan a whole table with `iter()`, so they read the current season's scores through this index.
- `season` has a single row, with `id` 0, that holds the current season's number and when it ends.
- `season_timer` is the schedule table that ends the season.
- `season_winner` holds the 3 best players of each finished season.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt, Timestamp } from 'spacetimedb';
import { schema, table, t, SenderError } from 'spacetimedb/server';

// How long each season lasts: 4 weeks, in microseconds.
const SEASON_LENGTH_MICROS = 28n * 24n * 60n * 60n * 1_000_000n;
// How many players of each season go into the hall of fame.
const WINNERS_PER_SEASON = 3;

const playerScore = table(
  { name: 'player_score' },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    // The season this score was set in.
    season: t.u32().index('btree'),
    score: t.u64(),
  }
);

// The current season, in a single row with id 0.
const season = table(
  { name: 'season' },
  {
    id: t.u8().primaryKey(),
    number: t.u32(),
    endsAt: t.timestamp(),
  }
);

const seasonTimer = table(
  { name: 'season_timer' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

// The best players of each finished season.
const seasonWinner = table(
  { name: 'season_winner' },
  {
    id: t.u64().primaryKey().autoInc(),
    season: t.u32(),
    rank: t.u8(),
    name: t.string(),
    score: t.u64(),
  }
);

const spacetimedb = schema({ playerScore, season, seasonTimer, seasonWinner });
export default spacetimedb;

// Sorts scores from highest to lowest.
function byScoreDescending(a: { score: bigint }, b: { score: bigint }): number {
  return a.score < b.score ? 1 : a.score > b.score ? -1 : 0;
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    // How long each season lasts: 4 weeks.
    static readonly TimeDuration SeasonLength = TimeDuration.FromDays(28);
    // How many players of each season go into the hall of fame.
    const int WinnersPerSeason = 3;

    [SpacetimeDB.Table(Accessor = "PlayerScore")]
    public partial struct PlayerScore
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;

        public string Name;

        // The season this score was set in.
        [SpacetimeDB.Index.BTree]
        public uint Season;

        public ulong Score;
    }

    // The current season, in a single row with id 0.
    [SpacetimeDB.Table(Accessor = "Season")]
    public partial struct Season
    {
        [SpacetimeDB.PrimaryKey]
        public byte Id;
        public uint Number;
        public Timestamp EndsAt;
    }

    [SpacetimeDB.Table(Accessor = "SeasonTimer", Scheduled = "EndSeason", ScheduledAt = "ScheduledAt")]
    public partial struct SeasonTimer
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }

    // The best players of each finished season.
    [SpacetimeDB.Table(Accessor = "SeasonWinner")]
    public partial struct SeasonWinner
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public uint Season;
        public byte Rank;
        public string Name;
        public ulong Score;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, view, AnonymousViewContext, Identity, Query, ReducerContext, ScheduleAt, Table, Timestamp};
use std::time::Duration;

// How long each season lasts: 4 weeks.
const SEASON_LENGTH: Duration = Duration::from_secs(28 * 24 * 60 * 60);
// How many players of each season go into the hall of fame.
const WINNERS_PER_SEASON: usize = 3;

#[table(accessor = player_score)]
pub struct PlayerScore {
    #[primary_key]
    identity: Identity,
    name: String,
    // The season this score was set in.
    #[index(btree)]
    season: u32,
    score: u64,
}

// The current season, in a single row with id 0.
#[table(accessor = season)]
pub struct Season {
    #[primary_key]
    id: u8,
    number: u32,
    ends_at: Timestamp,
}

#[table(accessor = season_timer, scheduled(end_season))]
pub struct SeasonTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

// The best players of each finished season.
#[table(accessor = season_winner)]
pub struct SeasonWinner {
    #[primary_key]
    #[auto_inc]
    id: u64,
    season: u32,
    rank: u8,
    name: String,
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

// How long each season lasts: 4 weeks.
const TimeDuration SEASON_LENGTH = TimeDuration::from_seconds(28 * 24 * 60 * 60);
// How many players of each season go into the hall of fame.
constexpr size_t WINNERS_PER_SEASON = 3;

struct PlayerScore {
    Identity identity;
    std::string name;
    // The season this score was set in.
    uint32_t season;
    uint64_t score;
};
SPACETIMEDB_STRUCT(PlayerScore, identity, name, season, score)
SPACETIMEDB_TABLE(PlayerScore, player_score, Private)
FIELD_PrimaryKey(player_score, identity)
FIELD_Index(player_score, season)

// The current season, in a single row with id 0.
struct Season {
    uint8_t id;
    uint32_t number;
    Timestamp ends_at;
};
SPACETIMEDB_STRUCT(Season, id, number, ends_at)
SPACETIMEDB_TABLE(Season, season, Private)
FIELD_PrimaryKey(season, id)

struct SeasonTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
};
SPACETIMEDB_STRUCT(SeasonTimer, scheduled_id, scheduled_at)
SPACETIMEDB_TABLE(SeasonTimer, season_timer, Private)
FIELD_PrimaryKeyAutoInc(season_timer, scheduled_id)
SPACETIMEDB_SCHEDULE(season_timer, 1, end_season)  // Column 1 is scheduled_at

// The best players of each finished season.
struct SeasonWinner {
    uint64_t id;
    uint32_t season;
    uint8_t rank;
    std::string name;
    uint64_t score;
};
SPACETIMEDB_STRUCT(SeasonWinner, id, season, rank, name, score)
SPACETIMEDB_TABLE(SeasonWinner, season_winner, Private)
FIELD_PrimaryKeyAutoInc(season_winner, id)

// Sorts scores from highest to lowest.
bool by_score_descending(const PlayerScore& a, const PlayerScore& b) {
    return a.score > b.score;
}
```

</TabItem>
</Tabs>

## Start the first season

Insert season 1 and its timer from the `init` reducer. The timer fires once, when the season ends.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const init = spacetimedb.init(ctx => {
  const endsAt = new Timestamp(ctx.timestamp.microsSinceUnixEpoch + SEASON_LENGTH_MICROS);
  ctx.db.season.insert({ id: 0, number: 1, endsAt });
  ctx.db.seasonTimer.insert({ scheduledId: 0n, scheduledAt: ScheduleAt.time(endsAt.microsSinceUnixEpoch) });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        var endsAt = ctx.Timestamp + SeasonLength;
        ctx.Db.Season.Insert(new Season { Id = 0, Number = 1, EndsAt = endsAt });
        ctx.Db.SeasonTimer.Insert(new SeasonTimer { ScheduledId = 0, ScheduledAt = new ScheduleAt.Time(endsAt) });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    let ends_at = ctx.timestamp + SEASON_LENGTH;
    ctx.db.season().insert(Season { id: 0, number: 1, ends_at });
    ctx.db.season_timer().insert(SeasonTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ends_at),
    });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_INIT(init, ReducerContext ctx) {
    Timestamp ends_at = ctx.timestamp + SEASON_LENGTH;
    ctx.db[season].insert(Season{0, 1, ends_at});
    ctx.db[season_timer].insert(SeasonTimer{0, ScheduleAt(ends_at)});
    return Ok();
}
```

</TabItem>
</Tabs>

:::note
`init` only runs when the database is created. Publishing new code to an existing database doesn't run it again. To add seasons to a database that already exists, insert the `season` row and its timer from another reducer, once.
:::

## Submit a score

Add a `submit_score` reducer that players call with their name and score. It keeps each player's best score of the current season, and ignores lower ones. If the player's stored score is from an earlier season, the new score replaces it, even if it's lower: a player's first score of a season always counts.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const submitScore = spacetimedb.reducer(
  { name: t.string(), score: t.u64() },
  (ctx, { name, score }) => {
    const current = ctx.db.season.id.find(0);
    if (!current) {
      throw new SenderError('no season in progress');
    }
    const existing = ctx.db.playerScore.identity.find(ctx.sender);
    if (!existing) {
      ctx.db.playerScore.insert({ identity: ctx.sender, name, season: current.number, score });
    } else if (existing.season !== current.number || score > existing.score) {
      // Keep the player's best score of the season.
      // A score from an earlier season is replaced, even by a lower one.
      ctx.db.playerScore.identity.update({ ...existing, name, season: current.number, score });
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
        var current = ctx.Db.Season.Id.Find(0) ?? throw new Exception("No season in progress");
        if (ctx.Db.PlayerScore.Identity.Find(ctx.Sender) is not PlayerScore existing)
        {
            ctx.Db.PlayerScore.Insert(new PlayerScore
            {
                Identity = ctx.Sender,
                Name = name,
                Season = current.Number,
                Score = score,
            });
        }
        else if (existing.Season != current.Number || score > existing.Score)
        {
            // Keep the player's best score of the season.
            // A score from an earlier season is replaced, even by a lower one.
            existing.Name = name;
            existing.Season = current.Number;
            existing.Score = score;
            ctx.Db.PlayerScore.Identity.Update(existing);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn submit_score(ctx: &ReducerContext, name: String, score: u64) -> Result<(), String> {
    let season = ctx.db.season().id().find(0).ok_or("no season in progress")?.number;
    let table = ctx.db.player_score();
    match table.identity().find(ctx.sender()) {
        // Keep the player's best score of the season.
        Some(existing) if existing.season == season && existing.score >= score => {}
        // A score from an earlier season is replaced, even by a lower one.
        Some(existing) => {
            table.identity().update(PlayerScore {
                name,
                season,
                score,
                ..existing
            });
        }
        None => {
            table.insert(PlayerScore {
                identity: ctx.sender(),
                name,
                season,
                score,
            });
        }
    }
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(submit_score, ReducerContext ctx, std::string name, uint64_t score) {
    auto current = ctx.db[season_id].find(uint8_t(0));
    if (!current) {
        return Err("no season in progress");
    }
    auto existing = ctx.db[player_score_identity].find(ctx.sender());
    if (!existing) {
        ctx.db[player_score].insert(PlayerScore{ctx.sender(), name, current->number, score});
    } else if (existing->season != current->number || score > existing->score) {
        // Keep the player's best score of the season.
        // A score from an earlier season is replaced, even by a lower one.
        PlayerScore updated = *existing;
        updated.name = name;
        updated.season = current->number;
        updated.score = score;
        ctx.db[player_score_identity].update(updated);
    }
    return Ok();
}
```

</TabItem>
</Tabs>

## End a season

Add the `end_season` reducer, which SpacetimeDB calls when the season's timer fires. It does two things:

1. **It records the season's best players.** It reads the season's scores through the `season` index, sorts them from highest to lowest, and copies the first 3 into `season_winner`. Always sort the rows yourself rather than relying on the order of the index: the rows written by the current transaction can come out of the index before all the others.
2. **It starts the next season.** It increments the season number, and schedules the next end one season after the previous end, rather than one season after now. A timer can fire a few milliseconds late, and counting from now would make each season start a little later than the last.

`end_season` reads all of the season's scores, but only once per season. Clients can't call it: scheduled reducers are private, so only the scheduler, the database owner and team collaborators can run them.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const endSeason = spacetimedb.reducer(
  { onSchedule: seasonTimer },
  { arg: seasonTimer.rowType },
  ctx => {
    const current = ctx.db.season.id.find(0);
    if (!current) {
      throw new Error('no season in progress');
    }

    // Record the season's best players.
    const best = Array.from(ctx.db.playerScore.season.filter(current.number))
      .sort(byScoreDescending)
      .slice(0, WINNERS_PER_SEASON);
    best.forEach((row, i) => {
      ctx.db.seasonWinner.insert({
        id: 0n,
        season: current.number,
        rank: i + 1,
        name: row.name,
        score: row.score,
      });
    });

    // Start the next season. This season's scores stay in the table, but no longer count.
    const endsAt = new Timestamp(current.endsAt.microsSinceUnixEpoch + SEASON_LENGTH_MICROS);
    ctx.db.seasonTimer.insert({ scheduledId: 0n, scheduledAt: ScheduleAt.time(endsAt.microsSinceUnixEpoch) });
    ctx.db.season.id.update({ ...current, number: current.number + 1, endsAt });
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void EndSeason(ReducerContext ctx, SeasonTimer timer)
    {
        var current = ctx.Db.Season.Id.Find(0) ?? throw new Exception("No season in progress");

        // Record the season's best players.
        var best = ctx.Db.PlayerScore.Season.Filter(current.Number)
            .OrderByDescending(row => row.Score)
            .Take(WinnersPerSeason)
            .ToList();
        for (var i = 0; i < best.Count; i++)
        {
            ctx.Db.SeasonWinner.Insert(new SeasonWinner
            {
                Id = 0,
                Season = current.Number,
                Rank = (byte)(i + 1),
                Name = best[i].Name,
                Score = best[i].Score,
            });
        }

        // Start the next season. This season's scores stay in the table, but no longer count.
        current.Number += 1;
        current.EndsAt = current.EndsAt + SeasonLength;
        ctx.Db.SeasonTimer.Insert(new SeasonTimer { ScheduledId = 0, ScheduledAt = new ScheduleAt.Time(current.EndsAt) });
        ctx.Db.Season.Id.Update(current);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn end_season(ctx: &ReducerContext, _timer: SeasonTimer) -> Result<(), String> {
    let mut season = ctx.db.season().id().find(0).ok_or("no season in progress")?;

    // Record the season's best players.
    let mut scores: Vec<PlayerScore> = ctx.db.player_score().season().filter(season.number).collect();
    scores.sort_by(|a, b| b.score.cmp(&a.score));
    for (i, best) in scores.into_iter().take(WINNERS_PER_SEASON).enumerate() {
        ctx.db.season_winner().insert(SeasonWinner {
            id: 0,
            season: season.number,
            rank: i as u8 + 1,
            name: best.name,
            score: best.score,
        });
    }

    // Start the next season. This season's scores stay in the table, but no longer count.
    season.number += 1;
    season.ends_at = season.ends_at + SEASON_LENGTH;
    ctx.db.season_timer().insert(SeasonTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(season.ends_at),
    });
    ctx.db.season().id().update(season);
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(end_season, ReducerContext ctx, SeasonTimer timer) {
    auto found = ctx.db[season_id].find(uint8_t(0));
    if (!found) {
        return Err("no season in progress");
    }
    Season current = *found;

    // Record the season's best players.
    auto scores = ctx.db[player_score_season].filter(current.number).collect();
    std::sort(scores.begin(), scores.end(), by_score_descending);
    for (size_t i = 0; i < scores.size() && i < WINNERS_PER_SEASON; ++i) {
        ctx.db[season_winner].insert(SeasonWinner{
            0, current.number, static_cast<uint8_t>(i + 1), scores[i].name, scores[i].score});
    }

    // Start the next season. This season's scores stay in the table, but no longer count.
    current.number += 1;
    current.ends_at = current.ends_at + SEASON_LENGTH;
    ctx.db[season_timer].insert(SeasonTimer{0, ScheduleAt(current.ends_at)});
    ctx.db[season_id].update(current);
    return Ok();
}
```

</TabItem>
</Tabs>

## Define the views

Add three public anonymous views. They return the same rows to every player, so SpacetimeDB computes each one once and shares it with every subscribed client.

- `top_100` reads the current season's scores through the `season` index, sorts them from highest to lowest, and returns the first 100. Like `end_season`, it sorts the rows itself rather than relying on the order of the index.
- `current_season` returns the `season` row, so clients can show when the season ends.
- `hall_of_fame` returns every row of `season_winner`.

`current_season` and `hall_of_fame` return their whole table, so they return a query instead of rows: SpacetimeDB keeps them up to date the same way as a subscription query. Returning a whole table through a view keeps the table private, so you can still change what clients see later, for example to only show the last few seasons.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const LEADERBOARD_SIZE = 100;

export const top100 = spacetimedb.anonymousView(
  { name: 'top_100', public: true },
  t.array(playerScore.rowType),
  ctx => {
    const current = ctx.db.season.id.find(0);
    if (!current) {
      return [];
    }
    return Array.from(ctx.db.playerScore.season.filter(current.number))
      .sort(byScoreDescending)
      .slice(0, LEADERBOARD_SIZE);
  }
);

export const currentSeason = spacetimedb.anonymousView(
  { name: 'current_season', public: true },
  t.array(season.rowType),
  ctx => ctx.from.season
);

export const hallOfFame = spacetimedb.anonymousView(
  { name: 'hall_of_fame', public: true },
  t.array(seasonWinner.rowType),
  ctx => ctx.from.seasonWinner
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
        if (ctx.Db.Season.Id.Find(0) is not Season current)
        {
            return new List<PlayerScore>();
        }
        return ctx.Db.PlayerScore.Season.Filter(current.Number)
            .OrderByDescending(row => row.Score)
            .Take(LeaderboardSize)
            .ToList();
    }

    [SpacetimeDB.View(Accessor = "CurrentSeason", Public = true)]
    public static IQuery<Season> CurrentSeason(AnonymousViewContext ctx)
    {
        return ctx.From.Season();
    }

    [SpacetimeDB.View(Accessor = "HallOfFame", Public = true)]
    public static IQuery<SeasonWinner> HallOfFame(AnonymousViewContext ctx)
    {
        return ctx.From.SeasonWinner();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const LEADERBOARD_SIZE: usize = 100;

#[view(accessor = top_100, public)]
fn top_100(ctx: &AnonymousViewContext) -> Vec<PlayerScore> {
    let Some(season) = ctx.db.season().id().find(0) else {
        return vec![];
    };
    let mut scores: Vec<PlayerScore> = ctx.db.player_score().season().filter(season.number).collect();
    scores.sort_by(|a, b| b.score.cmp(&a.score));
    scores.truncate(LEADERBOARD_SIZE);
    scores
}

#[view(accessor = current_season, public)]
fn current_season(ctx: &AnonymousViewContext) -> impl Query<Season> {
    ctx.from.season()
}

#[view(accessor = hall_of_fame, public)]
fn hall_of_fame(ctx: &AnonymousViewContext) -> impl Query<SeasonWinner> {
    ctx.from.season_winner()
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
constexpr size_t LEADERBOARD_SIZE = 100;

SPACETIMEDB_VIEW(std::vector<PlayerScore>, top_100, Public, AnonymousViewContext ctx) {
    auto current = ctx.db[season_id].find(uint8_t(0));
    if (!current) {
        return {};
    }
    auto scores = ctx.db[player_score_season].filter(current->number).collect();
    std::sort(scores.begin(), scores.end(), by_score_descending);
    if (scores.size() > LEADERBOARD_SIZE) {
        scores.erase(scores.begin() + LEADERBOARD_SIZE, scores.end());
    }
    return scores;
}

SPACETIMEDB_VIEW(Query<Season>, current_season, Public, AnonymousViewContext ctx) {
    return ctx.from[season];
}

SPACETIMEDB_VIEW(Query<SeasonWinner>, hall_of_fame, Public, AnonymousViewContext ctx) {
    return ctx.from[season_winner];
}
```

</TabItem>
</Tabs>

## Show the season on the client

On the client, subscribe to the three views, and draw the season, the leaderboard and the hall of fame from the client cache each frame. The client cache doesn't keep the order the views returned their rows in, so sort the rows before you display them.

When a season ends, the client receives the whole change at once: the new season number, the hall of fame entries, and the removal of every score from the leaderboard.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.top100, tables.currentSeason, tables.hallOfFame]);
  })
  .build();

// Call this each frame to draw the season from the client cache.
function drawSeason() {
  for (const season of conn.db.currentSeason.iter()) {
    const daysLeft = Math.ceil((season.endsAt.toDate().getTime() - Date.now()) / 86_400_000);
    console.log(`Season ${season.number} ends in ${Math.max(daysLeft, 0)} days`);
  }

  // The client cache doesn't keep the view's order, so sort the rows here.
  const leaderboard = Array.from(conn.db.top100.iter()).sort((a, b) =>
    a.score < b.score ? 1 : a.score > b.score ? -1 : 0
  );
  leaderboard.forEach((row, i) => console.log(`${i + 1}. ${row.name} - ${row.score}`));

  // The hall of fame, most recent season first.
  const winners = Array.from(conn.db.hallOfFame.iter()).sort((a, b) => b.season - a.season || a.rank - b.rank);
  for (const winner of winners) {
    console.log(`Season ${winner.season}, #${winner.rank}: ${winner.name} - ${winner.score}`);
  }
}

// Call this when a game ends to submit the player's score.
function onGameOver(name: string, score: bigint) {
  conn.reducers.submitScore({ name, score }).catch(console.error);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.Top100())
            .AddQuery(q => q.From.CurrentSeason())
            .AddQuery(q => q.From.HallOfFame())
            .Subscribe();
    })
    .Build();

// Call this each frame to draw the season from the client cache.
void DrawSeason()
{
    foreach (var season in conn.Db.CurrentSeason.Iter())
    {
        var daysLeft = Math.Ceiling(((DateTimeOffset)season.EndsAt - DateTimeOffset.UtcNow).TotalDays);
        Console.WriteLine($"Season {season.Number} ends in {Math.Max(daysLeft, 0)} days");
    }

    // The client cache doesn't keep the view's order, so sort the rows here.
    var leaderboard = conn.Db.Top100.Iter().OrderByDescending(row => row.Score).ToList();
    for (var i = 0; i < leaderboard.Count; i++)
    {
        Console.WriteLine($"{i + 1}. {leaderboard[i].Name} - {leaderboard[i].Score}");
    }

    // The hall of fame, most recent season first.
    var winners = conn.Db.HallOfFame.Iter().OrderByDescending(w => w.Season).ThenBy(w => w.Rank);
    foreach (var winner in winners)
    {
        Console.WriteLine($"Season {winner.Season}, #{winner.Rank}: {winner.Name} - {winner.Score}");
    }
}

// Call this when a game ends to submit the player's score.
void OnGameOver(string name, ulong score) => conn.Reducers.SubmitScore(name, score);
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the client cache stays up to date.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Table, Timestamp};

// Call this each frame to draw the season from the client cache.
fn draw_season(conn: &DbConnection) {
    for season in conn.db().current_season().iter() {
        let time_left = season.ends_at.duration_since(Timestamp::now()).unwrap_or_default();
        let days_left = (time_left.as_secs_f64() / 86_400.0).ceil();
        println!("Season {} ends in {} days", season.number, days_left);
    }

    // The client cache doesn't keep the view's order, so sort the rows here.
    let mut leaderboard: Vec<_> = conn.db().top_100().iter().collect();
    leaderboard.sort_by(|a, b| b.score.cmp(&a.score));
    for (i, row) in leaderboard.iter().enumerate() {
        println!("{}. {} - {}", i + 1, row.name, row.score);
    }

    // The hall of fame, most recent season first.
    let mut winners: Vec<_> = conn.db().hall_of_fame().iter().collect();
    winners.sort_by(|a, b| b.season.cmp(&a.season).then(a.rank.cmp(&b.rank)));
    for winner in winners {
        println!("Season {}, #{}: {} - {}", winner.season, winner.rank, winner.name, winner.score);
    }
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .add_query(|q| q.from.top_100())
                .add_query(|q| q.from.current_season())
                .add_query(|q| q.from.hall_of_fame())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here, and calls draw_season(&conn) each frame.
}

// Call this when a game ends to submit the player's score.
fn on_game_over(conn: &DbConnection, name: String, score: u64) {
    conn.reducers().submit_score(name, score).unwrap();
}
```

</TabItem>
</Tabs>

## What's next?

You now have a leaderboard that starts over every 4 weeks, and remembers each season's best players:

- Each score records the season it was set in, and the views only read the current season's scores.
- A one-shot timer ends each season, records its 3 best players, and schedules the end of the next season.
- Ending a season never deletes a score, so it takes the same time however many players you have.

To adapt it to your game:

- **Give out rewards.** `end_season` already knows the season's best players: grant their rewards in the same reducer, for example by inserting rows into a table of badges.
- **End seasons on calendar dates.** To end each season on the first day of the month, compute the next end date in `end_season` instead of adding a fixed length.
- **Clean up old scores.** Rows from players who stopped playing stay in `player_score`. If that matters, delete the rows of old seasons from another scheduled reducer, a few at a time.
- **Reset ratings too.** If you use [skill-based matchmaking](./00350-skill-based-matchmaking.md), you can also reset players' ratings in `end_season`, so that everyone starts the new season on equal terms.

To learn more about the features used here, see [Schedule Tables](../../00200-core-concepts/00300-tables/00500-schedule-tables.md), [Indexes](../../00200-core-concepts/00300-tables/00300-indexes.md) and [Views](../../00200-core-concepts/00200-functions/00500-views.md).

Next, you can [keep usernames unique and block offensive ones](../01100-social/00100-unique-usernames.md).
