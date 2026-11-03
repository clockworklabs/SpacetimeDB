---
title: Make a turn-based game with turn timeouts
slug: /how-to/games/turn-based-game
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll build a game of tic-tac-toe where each player has 30 seconds to make a move, and loses the game if they run out of time. The server checks every move, so players can't cheat. You'll use a [reducer](../../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md) to check and apply moves, and a [schedule table](../../00200-core-concepts/00300-tables/00500-schedule-tables.md) to end turns that take too long.

The same pattern works for any turn-based game, such as a card game or a strategy game: replace the board and the rules with your own.

## How it works

- A private `game` table holds each game: its players, its board, whose turn it is, and when that turn times out.
- A `make_move` reducer checks that a move is legal, applies it, then gives the turn to the other player.
- A private `turn_timer` schedule table holds one timer per game in progress. When the current turn's deadline passes, it calls the `turn_timeout` reducer.
- Each move replaces the game's timer with a new one. If the timer fires, the player whose turn it is forfeits.
- A `my_games` view returns the caller's games. Clients subscribe to it, and show the board and a countdown.

## Define the tables

Add two tables and a view:

- `game` has one row per game. It's private: players read their games through the `my_games` view. It has an index on each player column. The view combines the two columns with `or`, and SpacetimeDB can't use an index for an `or`, so it reads every game. That's fine while the table is small. If you expect many games, split the view in two, one for each player column, as the [chat guide does for DMs](../01100-social/00300-chat-rooms-and-dms.md#show-each-player-their-messages): each view then reads only the caller's games through its index.
- `turn_timer` is the schedule table. It's private too: clients read the deadline from their games instead. Its `game_id` column is unique, so a game never has more than one timer.
- `my_games` is a public view that returns the caller's games, as X or as O. Keeping tables private and exposing what clients need through views is the recommended practice: see [Hide information from players](./00600-hide-information.md).

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt, Timestamp, type Identity } from 'spacetimedb';
import { schema, table, t, SenderError, type InferSchema, type ReducerCtx } from 'spacetimedb/server';

// How long each player has to make a move: 30 seconds.
const TURN_TIME_LIMIT_MICROS = 30_000_000n;

const game = table(
  { name: 'game' },
  {
    id: t.u64().primaryKey().autoInc(),
    playerX: t.identity().index('btree'),
    playerO: t.identity().index('btree'),
    // The 9 cells, row by row: 'X', 'O', or '-' when empty.
    board: t.string(),
    // The player whose turn it is.
    turn: t.identity(),
    // When the current turn times out.
    turnDeadline: t.timestamp(),
    finished: t.bool(),
    // The winner, or undefined while the game is in progress or after a draw.
    winner: t.option(t.identity()),
  }
);

const turnTimer = table(
  { name: 'turn_timer' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    gameId: t.u64().unique(),
  }
);

const spacetimedb = schema({ game, turnTimer });
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;

export const myGames = spacetimedb.view(
  { name: 'my_games', public: true },
  t.array(game.rowType),
  ctx => ctx.from.game.where(g => g.playerX.eq(ctx.sender).or(g.playerO.eq(ctx.sender)))
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    // How long each player has to make a move.
    static readonly TimeDuration TurnTimeLimit = TimeDuration.FromSeconds(30);

    [SpacetimeDB.Table(Accessor = "Game")]
    public partial struct Game
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Index.BTree]
        public Identity PlayerX;

        [SpacetimeDB.Index.BTree]
        public Identity PlayerO;

        // The 9 cells, row by row: 'X', 'O', or '-' when empty.
        public string Board;

        // The player whose turn it is.
        public Identity Turn;

        // When the current turn times out.
        public Timestamp TurnDeadline;

        public bool Finished;

        // The winner, or null while the game is in progress or after a draw.
        public Identity? Winner;
    }

    [SpacetimeDB.Table(Accessor = "TurnTimer", Scheduled = "TurnTimeout", ScheduledAt = "ScheduledAt")]
    public partial struct TurnTimer
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;

        [SpacetimeDB.Unique]
        public ulong GameId;
    }

    [SpacetimeDB.View(Accessor = "MyGames", Public = true)]
    public static IQuery<Game> MyGames(ViewContext ctx)
    {
        return ctx.From.Game().Where(g => g.PlayerX.Eq(ctx.Sender).Or(g.PlayerO.Eq(ctx.Sender)));
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, view, Identity, Query, ReducerContext, ScheduleAt, Table, Timestamp, ViewContext};
use std::time::Duration;

// How long each player has to make a move.
const TURN_TIME_LIMIT: Duration = Duration::from_secs(30);

#[table(accessor = game)]
pub struct Game {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    player_x: Identity,
    #[index(btree)]
    player_o: Identity,
    // The 9 cells, row by row: 'X', 'O', or '-' when empty.
    board: String,
    // The player whose turn it is.
    turn: Identity,
    // When the current turn times out.
    turn_deadline: Timestamp,
    finished: bool,
    // The winner, or None while the game is in progress or after a draw.
    winner: Option<Identity>,
}

#[table(accessor = turn_timer, scheduled(turn_timeout))]
pub struct TurnTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    #[unique]
    game_id: u64,
}

#[view(accessor = my_games, public)]
fn my_games(ctx: &ViewContext) -> impl Query<Game> {
    let me = ctx.sender();
    ctx.from.game().r#where(move |g| g.player_x.eq(me).or(g.player_o.eq(me)))
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
#include <algorithm>
#include <optional>
#include <string>
using namespace SpacetimeDB;

// How long each player has to make a move.
const TimeDuration TURN_TIME_LIMIT = TimeDuration::from_seconds(30);

struct Game {
    uint64_t id;
    Identity player_x;
    Identity player_o;
    // The 9 cells, row by row: 'X', 'O', or '-' when empty.
    std::string board;
    // The player whose turn it is.
    Identity turn;
    // When the current turn times out.
    Timestamp turn_deadline;
    bool finished;
    // The winner, or empty while the game is in progress or after a draw.
    std::optional<Identity> winner;
};
SPACETIMEDB_STRUCT(Game, id, player_x, player_o, board, turn, turn_deadline, finished, winner)
SPACETIMEDB_TABLE(Game, game, Private)
FIELD_PrimaryKeyAutoInc(game, id)
FIELD_Index(game, player_x)
FIELD_Index(game, player_o)

struct TurnTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    uint64_t game_id;
};
SPACETIMEDB_STRUCT(TurnTimer, scheduled_id, scheduled_at, game_id)
SPACETIMEDB_TABLE(TurnTimer, turn_timer, Private)
FIELD_PrimaryKeyAutoInc(turn_timer, scheduled_id)
FIELD_Unique(turn_timer, game_id)
SPACETIMEDB_SCHEDULE(turn_timer, 1, turn_timeout)  // Column 1 is scheduled_at

SPACETIMEDB_VIEW(Query<Game>, my_games, Public, ViewContext ctx) {
    Identity me = ctx.sender();
    return ctx.from[game].where([me](const auto& g) {
        return g.player_x.eq(me).or_(g.player_o.eq(me));
    });
}
```

</TabItem>
</Tabs>

## Start a game

Add two functions:

- `set_turn_timer` replaces the game's timer with one that fires once, at the turn's deadline. Starting a game and every move both use it.
- `start_game` creates a game between two players, gives the first turn to X, and starts the timer.

`start_game` isn't a reducer, so clients can't call it. Call it from your matchmaking when two players are matched. See the [matchmaking queue](./00300-matchmaking-queue.md) guide.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Replaces the game's timer with one that fires at `deadline`.
function setTurnTimer(ctx: Ctx, gameId: bigint, deadline: Timestamp) {
  ctx.db.turnTimer.gameId.delete(gameId);
  ctx.db.turnTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(deadline.microsSinceUnixEpoch),
    gameId,
  });
}

/** Starts a game between two players. Call it from your matchmaking. */
function startGame(ctx: Ctx, playerX: Identity, playerO: Identity) {
  const deadline = new Timestamp(ctx.timestamp.microsSinceUnixEpoch + TURN_TIME_LIMIT_MICROS);
  const row = ctx.db.game.insert({
    id: 0n,
    playerX,
    playerO,
    board: '-'.repeat(9),
    turn: playerX,
    turnDeadline: deadline,
    finished: false,
    winner: undefined,
  });
  setTurnTimer(ctx, row.id, deadline);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Replaces the game's timer with one that fires at `deadline`.
    static void SetTurnTimer(ReducerContext ctx, ulong gameId, Timestamp deadline)
    {
        ctx.Db.TurnTimer.GameId.Delete(gameId);
        ctx.Db.TurnTimer.Insert(new TurnTimer
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(deadline),
            GameId = gameId,
        });
    }

    /// Starts a game between two players. Call it from your matchmaking.
    public static void StartGame(ReducerContext ctx, Identity playerX, Identity playerO)
    {
        var deadline = ctx.Timestamp + TurnTimeLimit;
        var game = ctx.Db.Game.Insert(new Game
        {
            Id = 0,
            PlayerX = playerX,
            PlayerO = playerO,
            Board = new string('-', 9),
            Turn = playerX,
            TurnDeadline = deadline,
            Finished = false,
            Winner = null,
        });
        SetTurnTimer(ctx, game.Id, deadline);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Replaces the game's timer with one that fires at `deadline`.
fn set_turn_timer(ctx: &ReducerContext, game_id: u64, deadline: Timestamp) {
    ctx.db.turn_timer().game_id().delete(game_id);
    ctx.db.turn_timer().insert(TurnTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(deadline),
        game_id,
    });
}

/// Starts a game between two players. Call it from your matchmaking.
pub fn start_game(ctx: &ReducerContext, player_x: Identity, player_o: Identity) {
    let deadline = ctx.timestamp + TURN_TIME_LIMIT;
    let game = ctx.db.game().insert(Game {
        id: 0,
        player_x,
        player_o,
        board: "-".repeat(9),
        turn: player_x,
        turn_deadline: deadline,
        finished: false,
        winner: None,
    });
    set_turn_timer(ctx, game.id, deadline);
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// Replaces the game's timer with one that fires at `deadline`.
void set_turn_timer(ReducerContext& ctx, uint64_t game_id, Timestamp deadline) {
    ctx.db[turn_timer_game_id].delete_by_value(game_id);
    ctx.db[turn_timer].insert(TurnTimer{0, ScheduleAt(deadline), game_id});
}

// Starts a game between two players. Call it from your matchmaking.
void start_game(ReducerContext& ctx, Identity player_x, Identity player_o) {
    Timestamp deadline = ctx.timestamp + TURN_TIME_LIMIT;
    Game row = ctx.db[game].insert(
        Game{0, player_x, player_o, std::string(9, '-'), player_x, deadline, false, std::nullopt});
    set_turn_timer(ctx, row.id, deadline);
}
```

</TabItem>
</Tabs>

## Make a move

Add a `make_move` reducer. Clients can call it with any arguments, so it checks everything before it changes the board:

- The game exists and isn't over.
- It's the caller's turn. Players who aren't in the game never have the turn, so this also rejects them.
- The cell is on the board and empty.

It then places the caller's mark and looks for three in a row. If the caller won, or the board is full, the game is over and its timer is deleted. Otherwise, the turn goes to the other player, with a new deadline and a new timer.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// The cells of each row, column and diagonal.
const LINES = [
  [0, 1, 2],
  [3, 4, 5],
  [6, 7, 8],
  [0, 3, 6],
  [1, 4, 7],
  [2, 5, 8],
  [0, 4, 8],
  [2, 4, 6],
];

function opponent(row: { playerX: Identity; playerO: Identity }, player: Identity): Identity {
  return player.isEqual(row.playerX) ? row.playerO : row.playerX;
}

export const makeMove = spacetimedb.reducer(
  { gameId: t.u64(), cell: t.u8() },
  (ctx, { gameId, cell }) => {
    const row = ctx.db.game.id.find(gameId);
    if (!row) {
      throw new SenderError('game not found');
    }
    if (row.finished) {
      throw new SenderError('the game is over');
    }
    if (!row.turn.isEqual(ctx.sender)) {
      throw new SenderError('not your turn');
    }
    if (cell >= 9 || row.board[cell] !== '-') {
      throw new SenderError('invalid cell');
    }

    const mark = ctx.sender.isEqual(row.playerX) ? 'X' : 'O';
    const board = row.board.slice(0, cell) + mark + row.board.slice(cell + 1);
    const won = LINES.some(line => line.every(i => board[i] === mark));
    const full = !board.includes('-');

    const updated = { ...row, board };
    if (won || full) {
      updated.finished = true;
      updated.winner = won ? ctx.sender : undefined;
      ctx.db.turnTimer.gameId.delete(gameId);
    } else {
      updated.turn = opponent(row, ctx.sender);
      updated.turnDeadline = new Timestamp(ctx.timestamp.microsSinceUnixEpoch + TURN_TIME_LIMIT_MICROS);
      setTurnTimer(ctx, gameId, updated.turnDeadline);
    }
    ctx.db.game.id.update(updated);
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // The cells of each row, column and diagonal.
    static readonly int[][] Lines =
    [
        [0, 1, 2],
        [3, 4, 5],
        [6, 7, 8],
        [0, 3, 6],
        [1, 4, 7],
        [2, 5, 8],
        [0, 4, 8],
        [2, 4, 6],
    ];

    static Identity Opponent(Game game, Identity player) =>
        player == game.PlayerX ? game.PlayerO : game.PlayerX;

    [SpacetimeDB.Reducer]
    public static void MakeMove(ReducerContext ctx, ulong gameId, byte cell)
    {
        var game = ctx.Db.Game.Id.Find(gameId) ?? throw new Exception("Game not found");
        if (game.Finished)
        {
            throw new Exception("The game is over");
        }
        if (game.Turn != ctx.Sender)
        {
            throw new Exception("Not your turn");
        }
        if (cell >= 9 || game.Board[cell] != '-')
        {
            throw new Exception("Invalid cell");
        }

        var mark = ctx.Sender == game.PlayerX ? 'X' : 'O';
        var cells = game.Board.ToCharArray();
        cells[cell] = mark;
        var won = Lines.Any(line => line.All(i => cells[i] == mark));
        var full = !cells.Contains('-');
        game.Board = new string(cells);

        if (won || full)
        {
            game.Finished = true;
            game.Winner = won ? ctx.Sender : null;
            ctx.Db.TurnTimer.GameId.Delete(gameId);
        }
        else
        {
            game.Turn = Opponent(game, ctx.Sender);
            game.TurnDeadline = ctx.Timestamp + TurnTimeLimit;
            SetTurnTimer(ctx, gameId, game.TurnDeadline);
        }
        ctx.Db.Game.Id.Update(game);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// The cells of each row, column and diagonal.
const LINES: [[usize; 3]; 8] = [
    [0, 1, 2],
    [3, 4, 5],
    [6, 7, 8],
    [0, 3, 6],
    [1, 4, 7],
    [2, 5, 8],
    [0, 4, 8],
    [2, 4, 6],
];

fn opponent(game: &Game, player: Identity) -> Identity {
    if player == game.player_x {
        game.player_o
    } else {
        game.player_x
    }
}

#[reducer]
pub fn make_move(ctx: &ReducerContext, game_id: u64, cell: u8) -> Result<(), String> {
    let mut game = ctx.db.game().id().find(game_id).ok_or("game not found")?;
    if game.finished {
        return Err("the game is over".to_string());
    }
    if game.turn != ctx.sender() {
        return Err("not your turn".to_string());
    }
    let cell = cell as usize;
    if cell >= 9 || game.board.as_bytes()[cell] != b'-' {
        return Err("invalid cell".to_string());
    }

    let mark = if ctx.sender() == game.player_x { b'X' } else { b'O' };
    let mut cells = game.board.into_bytes();
    cells[cell] = mark;
    let won = LINES.iter().any(|line| line.iter().all(|&i| cells[i] == mark));
    let full = !cells.contains(&b'-');
    game.board = String::from_utf8(cells).unwrap();

    if won || full {
        game.finished = true;
        game.winner = won.then(|| ctx.sender());
        ctx.db.turn_timer().game_id().delete(game_id);
    } else {
        game.turn = opponent(&game, ctx.sender());
        game.turn_deadline = ctx.timestamp + TURN_TIME_LIMIT;
        set_turn_timer(ctx, game_id, game.turn_deadline);
    }
    ctx.db.game().id().update(game);
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// The cells of each row, column and diagonal.
constexpr int LINES[8][3] = {
    {0, 1, 2}, {3, 4, 5}, {6, 7, 8},
    {0, 3, 6}, {1, 4, 7}, {2, 5, 8},
    {0, 4, 8}, {2, 4, 6},
};

Identity opponent(const Game& row, const Identity& player) {
    return player == row.player_x ? row.player_o : row.player_x;
}

SPACETIMEDB_REDUCER(make_move, ReducerContext ctx, uint64_t game_id, uint8_t cell) {
    // `game_id` is the argument here, so the table's primary key index is `::game_id`.
    auto found = ctx.db[::game_id].find(game_id);
    if (!found) {
        return Err("game not found");
    }
    Game row = *found;
    if (row.finished) {
        return Err("the game is over");
    }
    if (row.turn != ctx.sender()) {
        return Err("not your turn");
    }
    if (cell >= 9 || row.board[cell] != '-') {
        return Err("invalid cell");
    }

    char mark = ctx.sender() == row.player_x ? 'X' : 'O';
    row.board[cell] = mark;
    bool won = std::any_of(std::begin(LINES), std::end(LINES), [&](const int* line) {
        return row.board[line[0]] == mark && row.board[line[1]] == mark && row.board[line[2]] == mark;
    });
    bool full = row.board.find('-') == std::string::npos;

    if (won || full) {
        row.finished = true;
        if (won) {
            row.winner = ctx.sender();
        }
        ctx.db[turn_timer_game_id].delete_by_value(game_id);
    } else {
        row.turn = opponent(row, ctx.sender());
        row.turn_deadline = ctx.timestamp + TURN_TIME_LIMIT;
        set_turn_timer(ctx, game_id, row.turn_deadline);
    }
    ctx.db[::game_id].update(row);
    return Ok();
}
```

</TabItem>
</Tabs>

A reducer runs as a single transaction, so a move and a timeout can never both happen, even when the move arrives at the last moment:

- If the move commits first, it replaces the timer. SpacetimeDB never calls `turn_timeout` for a timer that was deleted.
- If the timeout commits first, the game is over, and the move is rejected.

## End turns that take too long

Add the `turn_timeout` reducer. SpacetimeDB calls it when a game's timer fires, and the player whose turn it is forfeits: the other player wins.

Clients can't call `turn_timeout` themselves: scheduled reducers are private, so only the scheduler, the database owner and team collaborators can run them. That's what stops a player from ending their opponent's turn early.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const turnTimeout = spacetimedb.reducer(
  { onSchedule: turnTimer },
  { arg: turnTimer.rowType },
  (ctx, { arg }) => {
    const row = ctx.db.game.id.find(arg.gameId);
    if (!row || row.finished) {
      return;
    }
    // The player who ran out of time forfeits.
    ctx.db.game.id.update({
      ...row,
      finished: true,
      winner: opponent(row, row.turn),
    });
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void TurnTimeout(ReducerContext ctx, TurnTimer timer)
    {
        if (ctx.Db.Game.Id.Find(timer.GameId) is not Game game || game.Finished)
        {
            return;
        }
        // The player who ran out of time forfeits.
        game.Finished = true;
        game.Winner = Opponent(game, game.Turn);
        ctx.Db.Game.Id.Update(game);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn turn_timeout(ctx: &ReducerContext, timer: TurnTimer) {
    let Some(mut game) = ctx.db.game().id().find(timer.game_id) else {
        return;
    };
    if game.finished {
        return;
    }
    // The player who ran out of time forfeits.
    game.finished = true;
    game.winner = Some(opponent(&game, game.turn));
    ctx.db.game().id().update(game);
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(turn_timeout, ReducerContext ctx, TurnTimer timer) {
    auto found = ctx.db[game_id].find(timer.game_id);
    if (!found || found->finished) {
        return Ok();
    }
    // The player who ran out of time forfeits.
    Game row = *found;
    row.finished = true;
    row.winner = opponent(row, row.turn);
    ctx.db[game_id].update(row);
    return Ok();
}
```

</TabItem>
</Tabs>

A timer that fires once is deleted after it runs, so finished games don't leave timers behind.

You don't need to handle disconnects either: a player who closes the game runs out of time, and the other player wins.

## Play from the client

On the client, subscribe to `my_games`, and call `make_move` when the player clicks a cell.

Show the countdown from `turn_deadline`, and refresh it every frame in your game. The client's clock can be slightly off from the server's, so the countdown is only an estimate: the server decides when the turn ends.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';
import type { Game } from './module_bindings/types';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.myGames]);
  })
  .build();

function showGame(game: Game) {
  console.log([game.board.slice(0, 3), game.board.slice(3, 6), game.board.slice(6)].join('\n'));
  const me = conn.identity!;
  if (game.finished) {
    console.log(!game.winner ? 'Draw' : game.winner.isEqual(me) ? 'You won' : 'You lost');
  } else if (game.turn.isEqual(me)) {
    const secondsLeft = Math.ceil((game.turnDeadline.toDate().getTime() - Date.now()) / 1000);
    console.log(`Your turn: ${Math.max(secondsLeft, 0)} seconds left`);
  } else {
    console.log('Waiting for your opponent');
  }
}

conn.db.myGames.onInsert((_ctx, game) => showGame(game));
conn.db.myGames.onUpdate((_ctx, _old, game) => showGame(game));

// Call this when the player clicks a cell, numbered 0 to 8.
function play(gameId: bigint, cell: number) {
  conn.reducers.makeMove({ gameId, cell }).catch(console.error);
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
            .AddQuery(q => q.From.MyGames())
            .Subscribe();
    })
    .Build();

void ShowGame(Game game)
{
    Console.WriteLine($"{game.Board[..3]}\n{game.Board[3..6]}\n{game.Board[6..]}");
    if (game.Finished)
    {
        Console.WriteLine(game.Winner is null ? "Draw" : game.Winner == conn.Identity ? "You won" : "You lost");
    }
    else if (game.Turn == conn.Identity)
    {
        var timeLeft = (DateTimeOffset)game.TurnDeadline - DateTimeOffset.UtcNow;
        Console.WriteLine($"Your turn: {Math.Max(Math.Ceiling(timeLeft.TotalSeconds), 0)} seconds left");
    }
    else
    {
        Console.WriteLine("Waiting for your opponent");
    }
}

conn.Db.MyGames.OnInsert += (ctx, game) => ShowGame(game);
conn.Db.MyGames.OnUpdate += (ctx, oldGame, game) => ShowGame(game);

// Call this when the player clicks a cell, numbered 0 to 8.
void Play(ulong gameId, byte cell) => conn.Reducers.MakeMove(gameId, cell);
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Identity, Table, TableWithPrimaryKey, Timestamp};

fn show_game(game: &Game, me: Identity) {
    println!("{}\n{}\n{}", &game.board[..3], &game.board[3..6], &game.board[6..]);
    if game.finished {
        match game.winner {
            None => println!("Draw"),
            Some(winner) if winner == me => println!("You won"),
            Some(_) => println!("You lost"),
        }
    } else if game.turn == me {
        let time_left = game.turn_deadline.duration_since(Timestamp::now()).unwrap_or_default();
        println!("Your turn: {} seconds left", time_left.as_secs_f64().ceil());
    } else {
        println!("Waiting for your opponent");
    }
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .add_query(|q| q.from.my_games())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    conn.db().my_games().on_insert(|ctx, game| show_game(game, ctx.identity()));
    conn.db()
        .my_games()
        .on_update(|ctx, _old, game| show_game(game, ctx.identity()));

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

// Call this when the player clicks a cell, numbered 0 to 8.
fn play(conn: &DbConnection, game_id: u64, cell: u8) {
    conn.reducers().make_move(game_id, cell).unwrap();
}
```

</TabItem>
</Tabs>

## What's next?

You now have a turn-based game where the server enforces both the rules and the time limit:

- `make_move` rejects any move that isn't legal, then gives the turn to the other player.
- Each game has one timer that calls `turn_timeout` when a turn takes too long, and each move replaces it.
- Because each reducer is a transaction, a move and a timeout can never both happen.

To handle timeouts differently:

- **Skip the turn instead of forfeiting.** In `turn_timeout`, give the turn to the other player and call `set_turn_timer` with a new deadline, as `make_move` does. To end games where both players are gone, count each player's missed turns and end the game after a few.
- **Use a chess clock.** Give each player a time budget for the whole game, rather than a fixed time per turn. Store each player's remaining time in `game`, subtract the time each move took, and set the timer to the current player's remaining time.

In tic-tac-toe, players see the whole board, so `my_games` returns whole games. For games where players must not see everything, such as card games, see [Hide information from players](./00600-hide-information.md).

To learn more about the features used here, see [Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md), [Schedule Tables](../../00200-core-concepts/00300-tables/00500-schedule-tables.md) and [Views](../../00200-core-concepts/00200-functions/00500-views.md).

Next, you can learn to [only sync what's near the player](./00500-sync-nearby-entities.md).
