---
title: Skill-based matchmaking with OpenSkill
slug: /how-to/games/skill-based-matchmaking
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first. This guide builds on [Build a matchmaking queue](./00300-matchmaking-queue.md), so read that one first.
:::

In this guide, you'll put players of similar skill in the same match, and update each player's skill after every match. Skill is measured with [OpenSkill](https://openskill.me), an open-source rating system that, like TrueSkill, rates players in teams as well as one-on-one. Matches have two teams, and a single constant switches between 1v1 and 2v2. You'll use a [schedule table](../../00200-core-concepts/00300-tables/00500-schedule-tables.md) to look for matches every second, allowing a wider skill gap the longer players wait.

## How it works

- A public `player_rating` table holds each player's rating: `mu`, their estimated skill, and `sigma`, how uncertain that estimate is. New players start at a `mu` of 25 and a `sigma` of 8.33.
- A `join_queue` reducer adds the caller to the queue with their current `mu`.
- A `match_players` reducer runs every second. It sorts the waiting players by `mu`, and creates a match from any group of neighbours whose skill gap is small enough. The allowed gap grows with time spent waiting, so nobody waits forever.
- Each match has two teams. Players are dealt to the teams so that both get a similar total skill.
- A `finish_match` function, which your game logic calls when a match ends, updates the rating of every player in it.

## Install OpenSkill

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Add the [`openskill`](https://www.npmjs.com/package/openskill) package to your module:

```bash
npm install openskill
```

</TabItem>
<TabItem value="csharp" label="C#">

Add the [`OpenSkillSharp`](https://www.nuget.org/packages/OpenSkillSharp) package to your module:

```bash
dotnet add package OpenSkillSharp
```

</TabItem>
<TabItem value="rust" label="Rust">

Add the [`skillratings`](https://crates.io/crates/skillratings) crate to your module. Its `weng_lin` module implements the Weng-Lin algorithm, which is what OpenSkill is built on:

```bash
cargo add skillratings
```

</TabItem>
<TabItem value="cpp" label="C++">

There's no OpenSkill library for C++ yet. Instead, you'll write the rating update yourself: it's a short function, shown in [Update ratings when a match ends](#update-ratings-when-a-match-ends).

</TabItem>
</Tabs>

## Define the tables

Start from the tables of the matchmaking queue guide, and make three changes:

- `queue_entry` also stores the player's `mu`, so `match_players` can sort the queue without looking up every rating.
- `match_player` has a `team` column, 0 or 1.
- A new `player_rating` table holds every player's rating. It's public, so anyone can see anyone's rating.

Then add `matchmaking_timer`, the schedule table that calls `match_players`.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt } from 'spacetimedb';
import { schema, table, t, SenderError, type InferSchema, type ReducerCtx } from 'spacetimedb/server';
import { rate, rating } from 'openskill';

// Players on each team: 1 for 1v1 matches, 2 for 2v2, and so on.
const TEAM_SIZE = 2;
const MATCH_SIZE = 2 * TEAM_SIZE;

// The largest skill gap allowed in a match, and how much it grows
// for each second the longest-waiting player has waited.
const STARTING_GAP = 2;
const GAP_GROWTH_PER_SECOND = 0.2;

const playerRating = table(
  { name: 'player_rating', public: true },
  {
    identity: t.identity().primaryKey(),
    mu: t.f64(),
    sigma: t.f64(),
  }
);

const queueEntry = table(
  { name: 'queue_entry' },
  {
    identity: t.identity().primaryKey(),
    joinedAt: t.timestamp(),
    mu: t.f64(),
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
    team: t.u8(),
  }
);

const matchmakingTimer = table(
  { name: 'matchmaking_timer' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const spacetimedb = schema({ playerRating, queueEntry, gameMatch, matchPlayer, matchmakingTimer });
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;
using OpenSkillSharp.Models;
using OpenSkillSharp.Rating;

public static partial class Module
{
    // Players on each team: 1 for 1v1 matches, 2 for 2v2, and so on.
    const int TeamSize = 2;
    const int MatchSize = 2 * TeamSize;

    // The largest skill gap allowed in a match, and how much it grows
    // for each second the longest-waiting player has waited.
    const double StartingGap = 2;
    const double GapGrowthPerSecond = 0.2;

    // OpenSkill's default model.
    static readonly PlackettLuce Model = new();

    [SpacetimeDB.Table(Accessor = "PlayerRating", Public = true)]
    public partial struct PlayerRating
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public double Mu;
        public double Sigma;
    }

    [SpacetimeDB.Table(Accessor = "QueueEntry")]
    public partial struct QueueEntry
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public Timestamp JoinedAt;
        public double Mu;
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
        public byte Team;
    }

    [SpacetimeDB.Table(Accessor = "MatchmakingTimer", Scheduled = "MatchPlayers", ScheduledAt = "ScheduledAt")]
    public partial struct MatchmakingTimer
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use skillratings::weng_lin::{weng_lin_two_teams, WengLinConfig, WengLinRating};
use skillratings::Outcomes;
use spacetimedb::{reducer, table, Identity, ReducerContext, ScheduleAt, Table, Timestamp};
use std::time::Duration;

// Players on each team: 1 for 1v1 matches, 2 for 2v2, and so on.
const TEAM_SIZE: usize = 2;
const MATCH_SIZE: usize = 2 * TEAM_SIZE;

// The largest skill gap allowed in a match, and how much it grows
// for each second the longest-waiting player has waited.
const STARTING_GAP: f64 = 2.0;
const GAP_GROWTH_PER_SECOND: f64 = 0.2;

#[table(accessor = player_rating, public)]
pub struct PlayerRating {
    #[primary_key]
    identity: Identity,
    mu: f64,
    sigma: f64,
}

#[table(accessor = queue_entry)]
pub struct QueueEntry {
    #[primary_key]
    identity: Identity,
    joined_at: Timestamp,
    mu: f64,
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
    team: u8,
}

#[table(accessor = matchmaking_timer, scheduled(match_players))]
pub struct MatchmakingTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
#include <algorithm>
#include <cmath>
using namespace SpacetimeDB;

// Players on each team: 1 for 1v1 matches, 2 for 2v2, and so on.
constexpr size_t TEAM_SIZE = 2;
constexpr size_t MATCH_SIZE = 2 * TEAM_SIZE;

// The largest skill gap allowed in a match, and how much it grows
// for each second the longest-waiting player has waited.
constexpr double STARTING_GAP = 2.0;
constexpr double GAP_GROWTH_PER_SECOND = 0.2;

struct PlayerRating {
    Identity identity;
    double mu;
    double sigma;
};
SPACETIMEDB_STRUCT(PlayerRating, identity, mu, sigma)
SPACETIMEDB_TABLE(PlayerRating, player_rating, Public)
FIELD_PrimaryKey(player_rating, identity)

struct QueueEntry {
    Identity identity;
    Timestamp joined_at;
    double mu;
};
SPACETIMEDB_STRUCT(QueueEntry, identity, joined_at, mu)
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
    uint8_t team;
};
SPACETIMEDB_STRUCT(MatchPlayer, identity, match_id, team)
SPACETIMEDB_TABLE(MatchPlayer, match_player, Private)
FIELD_PrimaryKey(match_player, identity)
FIELD_Index(match_player, match_id)

struct MatchmakingTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
};
SPACETIMEDB_STRUCT(MatchmakingTimer, scheduled_id, scheduled_at)
SPACETIMEDB_TABLE(MatchmakingTimer, matchmaking_timer, Private)
FIELD_PrimaryKeyAutoInc(matchmaking_timer, scheduled_id)
SPACETIMEDB_SCHEDULE(matchmaking_timer, 1, match_players)  // Column 1 is scheduled_at
```

</TabItem>
</Tabs>

## Join the queue

Insert the timer row from the `init` reducer, so `match_players` runs every second. `init` only runs when the database is created. Publishing new code to an existing database doesn't run it again: to start matchmaking there, insert the timer row from another reducer, once.

Then change `join_queue`. It gives new players OpenSkill's default rating, and adds the caller to the queue with their current `mu`. It no longer creates matches: `match_players` does.

Keep `leave_queue`, `client_disconnected` and the `my_match` view from the matchmaking queue guide as they are.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const init = spacetimedb.init(ctx => {
  ctx.db.matchmakingTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(1_000_000n),
  });
});

export const joinQueue = spacetimedb.reducer(ctx => {
  if (ctx.db.queueEntry.identity.find(ctx.sender)) {
    throw new SenderError('already in the queue');
  }
  if (ctx.db.matchPlayer.identity.find(ctx.sender)) {
    throw new SenderError('already in a match');
  }
  let player = ctx.db.playerRating.identity.find(ctx.sender);
  if (!player) {
    const { mu, sigma } = rating();
    player = ctx.db.playerRating.insert({ identity: ctx.sender, mu, sigma });
  }
  ctx.db.queueEntry.insert({ identity: ctx.sender, joinedAt: ctx.timestamp, mu: player.mu });
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
        ctx.Db.MatchmakingTimer.Insert(new MatchmakingTimer
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Interval(TimeSpan.FromSeconds(1)),
        });
    }

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
        if (ctx.Db.PlayerRating.Identity.Find(ctx.Sender) is not PlayerRating player)
        {
            var initial = Model.Rating();
            player = ctx.Db.PlayerRating.Insert(new PlayerRating
            {
                Identity = ctx.Sender,
                Mu = initial.Mu,
                Sigma = initial.Sigma,
            });
        }
        ctx.Db.QueueEntry.Insert(new QueueEntry
        {
            Identity = ctx.Sender,
            JoinedAt = ctx.Timestamp,
            Mu = player.Mu,
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.matchmaking_timer().insert(MatchmakingTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_secs(1).into()),
    });
}

#[reducer]
pub fn join_queue(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.db.queue_entry().identity().find(ctx.sender()).is_some() {
        return Err("already in the queue".to_string());
    }
    if ctx.db.match_player().identity().find(ctx.sender()).is_some() {
        return Err("already in a match".to_string());
    }
    let player = match ctx.db.player_rating().identity().find(ctx.sender()) {
        Some(player) => player,
        None => {
            let initial = WengLinRating::new();
            ctx.db.player_rating().insert(PlayerRating {
                identity: ctx.sender(),
                mu: initial.rating,
                sigma: initial.uncertainty,
            })
        }
    };
    ctx.db.queue_entry().insert(QueueEntry {
        identity: ctx.sender(),
        joined_at: ctx.timestamp,
        mu: player.mu,
    });
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// OpenSkill's default rating for new players.
constexpr double DEFAULT_MU = 25.0;
constexpr double DEFAULT_SIGMA = 25.0 / 3.0;

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    ctx.db[matchmaking_timer].insert(MatchmakingTimer{0, ScheduleAt(TimeDuration::from_millis(1000))});
    return Ok();
}

SPACETIMEDB_REDUCER(join_queue, ReducerContext ctx) {
    if (ctx.db[queue_entry_identity].find(ctx.sender())) {
        return Err("already in the queue");
    }
    if (ctx.db[match_player_identity].find(ctx.sender())) {
        return Err("already in a match");
    }
    auto player = ctx.db[player_rating_identity].find(ctx.sender());
    if (!player) {
        player = ctx.db[player_rating].insert(PlayerRating{ctx.sender(), DEFAULT_MU, DEFAULT_SIGMA});
    }
    ctx.db[queue_entry].insert(QueueEntry{ctx.sender(), ctx.timestamp, player->mu});
    return Ok();
}
```

</TabItem>
</Tabs>

## Match players of similar skill

`match_players` sorts the waiting players by `mu`, so players of similar skill are next to each other. It then slides along the sorted list, looking at each group of neighbours big enough to fill a match:

- If the gap between the group's strongest and weakest player is small enough, it creates a match with them and moves past them.
- Otherwise, it moves on by one player.

The allowed gap starts at `STARTING_GAP` and grows with the time the group's longest-waiting player has waited. A player whose skill is far from everyone else's still gets a match, after waiting longer.

To create a match, it deals the players to the two teams in the order A, B, B, A, A, B, B, A, and so on. Because the players are sorted by skill, this gives both teams a similar total skill. In a 1v1 match, it simply puts one player on each team.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const matchPlayers = spacetimedb.reducer(
  { onSchedule: matchmakingTimer },
  { arg: matchmakingTimer.rowType },
  ctx => {
    const waiting = Array.from(ctx.db.queueEntry.iter()).sort((a, b) => a.mu - b.mu);
    let i = 0;
    while (i + MATCH_SIZE <= waiting.length) {
      const group = waiting.slice(i, i + MATCH_SIZE);
      const gap = group[MATCH_SIZE - 1].mu - group[0].mu;
      const longestWait = Math.max(
        ...group.map(entry => Number(ctx.timestamp.since(entry.joinedAt).micros) / 1_000_000)
      );
      if (gap > STARTING_GAP + GAP_GROWTH_PER_SECOND * longestWait) {
        i += 1;
        continue;
      }

      const game = ctx.db.gameMatch.insert({ id: 0n, createdAt: ctx.timestamp });
      group.forEach((entry, k) => {
        ctx.db.queueEntry.identity.delete(entry.identity);
        // Teams in the order A, B, B, A, A, B, B, A, ...
        const team = Math.floor((k + 1) / 2) % 2;
        ctx.db.matchPlayer.insert({ identity: entry.identity, matchId: game.id, team });
      });
      i += MATCH_SIZE;
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
    public static void MatchPlayers(ReducerContext ctx, MatchmakingTimer timer)
    {
        var waiting = ctx.Db.QueueEntry.Iter().OrderBy(e => e.Mu).ToList();
        var i = 0;
        while (i + MatchSize <= waiting.Count)
        {
            var group = waiting.GetRange(i, MatchSize);
            var gap = group[MatchSize - 1].Mu - group[0].Mu;
            var longestWait = group.Max(e => ctx.Timestamp.TimeDurationSince(e.JoinedAt).Microseconds / 1_000_000.0);
            if (gap > StartingGap + GapGrowthPerSecond * longestWait)
            {
                i += 1;
                continue;
            }

            var game = ctx.Db.GameMatch.Insert(new GameMatch { Id = 0, CreatedAt = ctx.Timestamp });
            for (var k = 0; k < MatchSize; k++)
            {
                ctx.Db.QueueEntry.Identity.Delete(group[k].Identity);
                ctx.Db.MatchPlayer.Insert(new MatchPlayer
                {
                    Identity = group[k].Identity,
                    MatchId = game.Id,
                    // Teams in the order A, B, B, A, A, B, B, A, ...
                    Team = (byte)((k + 1) / 2 % 2),
                });
            }
            i += MatchSize;
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn match_players(ctx: &ReducerContext, _timer: MatchmakingTimer) {
    let mut waiting: Vec<QueueEntry> = ctx.db.queue_entry().iter().collect();
    waiting.sort_by(|a, b| a.mu.total_cmp(&b.mu));

    let mut i = 0;
    while i + MATCH_SIZE <= waiting.len() {
        let group = &waiting[i..i + MATCH_SIZE];
        let gap = group[MATCH_SIZE - 1].mu - group[0].mu;
        let longest_wait = group
            .iter()
            .map(|entry| {
                ctx.timestamp
                    .duration_since(entry.joined_at)
                    .unwrap_or_default()
                    .as_secs_f64()
            })
            .fold(0.0, f64::max);
        if gap > STARTING_GAP + GAP_GROWTH_PER_SECOND * longest_wait {
            i += 1;
            continue;
        }

        let game = ctx.db.game_match().insert(GameMatch {
            id: 0,
            created_at: ctx.timestamp,
        });
        for (k, entry) in group.iter().enumerate() {
            ctx.db.queue_entry().identity().delete(entry.identity);
            ctx.db.match_player().insert(MatchPlayer {
                identity: entry.identity,
                match_id: game.id,
                // Teams in the order A, B, B, A, A, B, B, A, ...
                team: ((k + 1) / 2 % 2) as u8,
            });
        }
        i += MATCH_SIZE;
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(match_players, ReducerContext ctx, MatchmakingTimer timer) {
    std::vector<QueueEntry> waiting;
    for (const auto& entry : ctx.db[queue_entry]) {
        waiting.push_back(entry);
    }
    std::sort(waiting.begin(), waiting.end(), [](const QueueEntry& a, const QueueEntry& b) {
        return a.mu < b.mu;
    });

    size_t i = 0;
    while (i + MATCH_SIZE <= waiting.size()) {
        double gap = waiting[i + MATCH_SIZE - 1].mu - waiting[i].mu;
        double longest_wait = 0.0;
        for (size_t k = i; k < i + MATCH_SIZE; ++k) {
            int64_t waited = ctx.timestamp.micros_since_epoch() - waiting[k].joined_at.micros_since_epoch();
            longest_wait = std::max(longest_wait, waited / 1'000'000.0);
        }
        if (gap > STARTING_GAP + GAP_GROWTH_PER_SECOND * longest_wait) {
            i += 1;
            continue;
        }

        GameMatch game = ctx.db[game_match].insert(GameMatch{0, ctx.timestamp});
        for (size_t k = 0; k < MATCH_SIZE; ++k) {
            const QueueEntry& entry = waiting[i + k];
            ctx.db[queue_entry_identity].delete_by_key(entry.identity);
            // Teams in the order A, B, B, A, A, B, B, A, ...
            uint8_t team = (k + 1) / 2 % 2;
            ctx.db[match_player].insert(MatchPlayer{entry.identity, game.id, team});
        }
        i += MATCH_SIZE;
    }
    return Ok();
}
```

</TabItem>
</Tabs>

`match_players` sorts the whole queue every second, which stays fast with thousands of waiting players. It's a scheduled reducer, so clients can't call it.

:::tip Larger teams
The same code works for any team size: set `TEAM_SIZE` to 5 for 5v5 matches. The more players a match needs, the wider their skill gap tends to be, so players wait longer for a match. Raise `STARTING_GAP` or `GAP_GROWTH_PER_SECOND` to keep waits short.
:::

## Update ratings when a match ends

Add a `finish_match` function that takes a match and the team that won. It hands both teams' ratings to OpenSkill, winners first, saves the new ratings, then deletes the match so its players can join the queue again.

OpenSkill raises the winners' `mu` and lowers the losers'. The more surprising the result, the bigger the change: beating a much stronger team moves ratings more than beating a weaker one. Every match also lowers each player's `sigma`, as OpenSkill grows more certain of their skill.

`finish_match` is a plain function, not a reducer. Call it from your own game logic when a match ends, for example in the reducer that applies the winning move. If clients could call it, players could report wins they never earned.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
function finishMatch(ctx: Ctx, matchId: bigint, winningTeam: number) {
  const players = Array.from(ctx.db.matchPlayer.matchId.filter(matchId));
  const ratingOf = (identity: typeof ctx.sender) => ctx.db.playerRating.identity.find(identity)!;
  const winners = players.filter(p => p.team === winningTeam).map(p => ratingOf(p.identity));
  const losers = players.filter(p => p.team !== winningTeam).map(p => ratingOf(p.identity));

  // rate takes the teams in the order they finished.
  const [newWinners, newLosers] = rate([winners, losers]);
  winners.forEach((player, k) => ctx.db.playerRating.identity.update({ ...player, ...newWinners[k] }));
  losers.forEach((player, k) => ctx.db.playerRating.identity.update({ ...player, ...newLosers[k] }));

  for (const player of players) {
    ctx.db.matchPlayer.identity.delete(player.identity);
  }
  ctx.db.gameMatch.id.delete(matchId);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static void FinishMatch(ReducerContext ctx, ulong matchId, byte winningTeam)
    {
        var players = ctx.Db.MatchPlayer.MatchId.Filter(matchId).ToList();
        PlayerRating RatingOf(MatchPlayer p) => ctx.Db.PlayerRating.Identity.Find(p.Identity)!.Value;
        var winners = players.Where(p => p.Team == winningTeam).Select(RatingOf).ToList();
        var losers = players.Where(p => p.Team != winningTeam).Select(RatingOf).ToList();

        // Rate takes the teams in the order they finished.
        var result = Model.Rate([ToTeam(winners), ToTeam(losers)]).ToList();
        SaveRatings(ctx, winners, result[0]);
        SaveRatings(ctx, losers, result[1]);

        foreach (var player in players)
        {
            ctx.Db.MatchPlayer.Identity.Delete(player.Identity);
        }
        ctx.Db.GameMatch.Id.Delete(matchId);
    }

    static Team ToTeam(List<PlayerRating> players) =>
        new() { Players = players.Select(p => Model.Rating(p.Mu, p.Sigma)).ToList() };

    static void SaveRatings(ReducerContext ctx, List<PlayerRating> players, ITeam rated)
    {
        var newRatings = rated.Players.ToList();
        for (var k = 0; k < players.Count; k++)
        {
            var player = players[k];
            player.Mu = newRatings[k].Mu;
            player.Sigma = newRatings[k].Sigma;
            ctx.Db.PlayerRating.Identity.Update(player);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
fn finish_match(ctx: &ReducerContext, match_id: u64, winning_team: u8) {
    let players: Vec<MatchPlayer> = ctx.db.match_player().match_id().filter(match_id).collect();
    let mut winners = Vec::new();
    let mut losers = Vec::new();
    for player in &players {
        let rating = ctx
            .db
            .player_rating()
            .identity()
            .find(player.identity)
            .expect("every player in a match has a rating");
        if player.team == winning_team {
            winners.push(rating);
        } else {
            losers.push(rating);
        }
    }

    // Outcomes::WIN means the first team won.
    let (new_winners, new_losers) = weng_lin_two_teams(
        &to_weng_lin(&winners),
        &to_weng_lin(&losers),
        &Outcomes::WIN,
        &WengLinConfig::new(),
    );
    save_ratings(ctx, winners, new_winners);
    save_ratings(ctx, losers, new_losers);

    for player in players {
        ctx.db.match_player().identity().delete(player.identity);
    }
    ctx.db.game_match().id().delete(match_id);
}

fn to_weng_lin(players: &[PlayerRating]) -> Vec<WengLinRating> {
    players
        .iter()
        .map(|p| WengLinRating {
            rating: p.mu,
            uncertainty: p.sigma,
        })
        .collect()
}

fn save_ratings(ctx: &ReducerContext, players: Vec<PlayerRating>, new_ratings: Vec<WengLinRating>) {
    for (player, new) in players.into_iter().zip(new_ratings) {
        ctx.db.player_rating().identity().update(PlayerRating {
            mu: new.rating,
            sigma: new.uncertainty,
            ..player
        });
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

The `rate_match` function below is OpenSkill's default model, written out for two teams. It raises each winner's `mu` by how unlikely the win was, scaled by how uncertain that player's rating is, and lowers each loser's `mu` by the same rule.

```cpp
// OpenSkill's default settings.
constexpr double BETA = 25.0 / 6.0;
constexpr double KAPPA = 0.0001;

static void update_team(std::vector<PlayerRating>& team, double c, double gamma, double omega, double delta) {
    for (auto& player : team) {
        double sigma_sq = player.sigma * player.sigma;
        player.mu += sigma_sq / c * omega;
        player.sigma *= std::sqrt(std::max(1.0 - sigma_sq / (c * c) * gamma * delta, KAPPA));
    }
}

// Updates the ratings of a two-team match with OpenSkill's default model.
static void rate_match(std::vector<PlayerRating>& winners, std::vector<PlayerRating>& losers) {
    double winners_mu = 0, winners_sigma_sq = 0, losers_mu = 0, losers_sigma_sq = 0;
    for (const auto& player : winners) {
        winners_mu += player.mu;
        winners_sigma_sq += player.sigma * player.sigma;
    }
    for (const auto& player : losers) {
        losers_mu += player.mu;
        losers_sigma_sq += player.sigma * player.sigma;
    }

    double c = std::sqrt(winners_sigma_sq + losers_sigma_sq + 2 * BETA * BETA);
    // How likely the winners were to win, before the match.
    double p_win = 1.0 / (1.0 + std::exp((losers_mu - winners_mu) / c));
    double delta = p_win * (1.0 - p_win);
    update_team(winners, c, std::sqrt(winners_sigma_sq) / c, 1.0 - p_win, delta);
    update_team(losers, c, std::sqrt(losers_sigma_sq) / c, -(1.0 - p_win), delta);
}

void finish_match(ReducerContext& ctx, uint64_t match_id, uint8_t winning_team) {
    auto players = ctx.db[match_player_match_id].filter(match_id).collect();
    std::vector<PlayerRating> winners, losers;
    for (const auto& player : players) {
        PlayerRating rating = *ctx.db[player_rating_identity].find(player.identity);
        (player.team == winning_team ? winners : losers).push_back(rating);
    }

    rate_match(winners, losers);
    for (const auto& player : winners) {
        ctx.db[player_rating_identity].update(player);
    }
    for (const auto& player : losers) {
        ctx.db[player_rating_identity].update(player);
    }

    for (const auto& player : players) {
        ctx.db[match_player_identity].delete_by_key(player.identity);
    }
    ctx.db[game_match_id].delete_by_key(match_id);
}
```

</TabItem>
</Tabs>

OpenSkill can also rate draws and matches with more than two teams. See the documentation of [openskill.js](https://github.com/philihp/openskill.js), [OpenSkillSharp](https://github.com/myssto/OpenSkillSharp) or [skillratings](https://docs.rs/skillratings) for details.

## Show the player's rating and rank

On the client, subscribe to the player's own row in `player_rating`, along with `my_match` to learn about new matches, as in the matchmaking queue guide.

`mu` alone is a poor rating to show players: a new player who wins their first match jumps straight to the top. Instead, show `mu - 3 * sigma`, which OpenSkill calls the ordinal. It's a cautious estimate of the player's skill: it starts near 0, and climbs as the player wins and OpenSkill grows more certain of their skill.

To show a rank such as Bronze or Gold, split that rating into tiers. Because the rating starts near 0, every new player starts in Bronze and climbs from there. Adjust the tiers to your game: few players get above 35.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect((ctx, identity) => {
    ctx
      .subscriptionBuilder()
      .subscribe([tables.playerRating.where(r => r.identity.eq(identity)), tables.myMatch]);
  })
  .build();

function rankOf(rating: number): string {
  if (rating >= 35) return 'Diamond';
  if (rating >= 25) return 'Platinum';
  if (rating >= 15) return 'Gold';
  if (rating >= 5) return 'Silver';
  return 'Bronze';
}

function showRating(player: { mu: number; sigma: number }) {
  const rating = player.mu - 3 * player.sigma;
  console.log(`Your rating: ${rating.toFixed(1)} (${rankOf(rating)})`);
}

// The player's row is inserted when they first join the queue,
// then updated after each match.
conn.db.playerRating.onInsert((_ctx, player) => showRating(player));
conn.db.playerRating.onUpdate((_ctx, _old, player) => showRating(player));
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
            .AddQuery(q => q.From.PlayerRating().Where(r => r.Identity.Eq(identity)))
            .AddQuery(q => q.From.MyMatch())
            .Subscribe();
    })
    .Build();

string RankOf(double rating) => rating switch
{
    >= 35 => "Diamond",
    >= 25 => "Platinum",
    >= 15 => "Gold",
    >= 5 => "Silver",
    _ => "Bronze",
};

void ShowRating(PlayerRating player)
{
    var rating = player.Mu - 3 * player.Sigma;
    Console.WriteLine($"Your rating: {rating:F1} ({RankOf(rating)})");
}

// The player's row is inserted when they first join the queue,
// then updated after each match.
conn.Db.PlayerRating.OnInsert += (ctx, player) => ShowRating(player);
conn.Db.PlayerRating.OnUpdate += (ctx, oldPlayer, player) => ShowRating(player);
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Table, TableWithPrimaryKey};

fn rank_of(rating: f64) -> &'static str {
    match rating {
        r if r >= 35.0 => "Diamond",
        r if r >= 25.0 => "Platinum",
        r if r >= 15.0 => "Gold",
        r if r >= 5.0 => "Silver",
        _ => "Bronze",
    }
}

fn show_rating(player: &PlayerRating) {
    let rating = player.mu - 3.0 * player.sigma;
    println!("Your rating: {rating:.1} ({})", rank_of(rating));
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, identity, _token| {
            ctx.subscription_builder()
                .add_query(move |q| q.from.player_rating().r#where(|r| r.identity.eq(identity)))
                .add_query(|q| q.from.my_match())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // The player's row is inserted when they first join the queue,
    // then updated after each match.
    conn.db().player_rating().on_insert(|_ctx, player| show_rating(player));
    conn.db()
        .player_rating()
        .on_update(|_ctx, _old, player| show_rating(player));

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}
```

</TabItem>
</Tabs>

## What's next?

You now have matchmaking that puts players of similar skill together, and learns their skill from every match:

- `player_rating` stores each player's OpenSkill rating, and `finish_match` updates it when a match ends, from your game logic only.
- `match_players` runs every second, matches neighbours in skill, and widens the allowed gap the longer players wait.
- `TEAM_SIZE` switches between 1v1 and team matches, and teams are dealt so their total skill is similar.
- Clients show a cautious rating, `mu - 3 * sigma`, and a rank based on it.

To support several game modes, such as 1v1 and 5v5, give each mode its own queue and its own ratings: a player's skill in one mode says little about the other.

- Add a `mode` column to `queue_entry`, pass the mode to `join_queue`, and have `match_players` match each mode separately, with that mode's team size.
- Add a `mode` column to `player_rating`. A player now has one row per mode, so `identity` can't be the primary key anymore: use an auto-incremented `id` instead, and a [multi-column index](../../00200-core-concepts/00300-tables/00300-indexes.md#multi-column-indexes) on `identity` and `mode` to find a player's rating in a mode.

To learn more about the features used here, see [Schedule Tables](../../00200-core-concepts/00300-tables/00500-schedule-tables.md), [Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md) and [Subscriptions](../../00200-core-concepts/00400-subscriptions.md).

To show the best players, sort them by `mu - 3 * sigma` in a [top 100 leaderboard](./00100-top-100-leaderboard.md). Next, you can [make a turn-based game with turn timeouts](./00400-turn-based-game.md).
