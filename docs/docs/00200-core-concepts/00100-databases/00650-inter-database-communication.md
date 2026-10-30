---
title: Inter-Database Communication
slug: /databases/inter-database-communication
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Asynchronous inter-database communication, or **async IDC**, lets a reducer in one database call a reducer in another. The calling reducer inserts a row into an **outbox table**. After its transaction commits, SpacetimeDB delivers the row to the other database as a call to the reducer that the outbox names. The calling reducer doesn't wait for that call to run. To learn how the call turned out, the calling module can declare an `on_result` reducer, which SpacetimeDB runs in the calling database after the other database has run the call.

This page calls the database that inserts the row the **calling database**, and the database whose reducer runs the **receiving database**. The receiving reducer is an ordinary reducer. Nothing marks it as callable from other databases, and the receiving module needs no changes to accept calls.

:::note
Outbox tables can be declared in Rust and TypeScript modules. A database with a Rust module can call one with a TypeScript module, and the other way around.
:::

## Why databases call each other

Applications often grow past a single database. A game might run a lobby and a separate database for each game world, so that each world scales on its own and a busy world doesn't slow down the others. A company might keep accounts, billing, and its product in separate databases that different teams own and publish on their own schedules. These databases still need to tell each other things: the lobby sends players into a world, and the product tells billing that a customer upgraded.

Reducers can't make network requests, so without async IDC, these messages have to leave the transaction. A client, an external service, or a [procedure](../00200-functions/00400-procedures.md) has to call the other database, outside of any transaction. If the caller crashes after the lobby removes a player but before the world admits them, the player is lost. If it retries after a timeout that hid a success, the world admits the player twice. Each caller has to handle retries, deduplication, ordering, and credentials on its own.

## What async IDC guarantees

An outbox makes a call to another database part of the calling transaction. SpacetimeDB then delivers the call with these guarantees:

- **Atomic with the caller.** An outbox row is part of the transaction that inserts it. If the transaction commits, SpacetimeDB sends the call. If it rolls back, nothing is sent. A reducer can remove a player from the lobby and send them to a world, and the two can't come apart.
- **Exactly once.** SpacetimeDB retries delivery until the receiving database answers, and the receiving database discards duplicates, so the receiving reducer runs once for each row.
- **In order.** Calls through one outbox table to one database run in the order their rows were inserted. Calls through different outbox tables, or to different databases, don't wait for each other.
- **Durable first.** SpacetimeDB sends a call only after the transaction that inserted it is durable, so the receiving database never acts on a call from a transaction that the calling database could lose in a crash.
- **Non-blocking.** The calling reducer finishes as soon as its transaction commits. The receiving database doesn't need to be available; calls wait until it is.
- **Authenticated.** In the receiving reducer, `ctx.sender` is the calling database's identity. SpacetimeDB sets it from the authenticated delivery, so one database can't pose as another.
- **Type-checked.** The calling module compiles against bindings generated from the receiving module, so a misspelled parameter or a wrong type is a compile error. If the receiving reducer's parameters change later, calls built from the old bindings fail with an error instead of being read with the wrong types.

Together, these let you split an application into databases that scale, deploy, and fail independently, while each call between them is sent if and only if its transaction commits, and runs exactly once, in order.

Async IDC doesn't make two databases commit together. The receiving reducer runs later, in its own transaction, and it can fail after the calling transaction has committed. When that matters, use `on_result` to undo or record the work in the calling database, as the example below does.

## Example: send players from a lobby to a game world

This example has two databases. Players wait in `lobby` and play in `game-world`. The lobby removes a player from its waiting list and sends them to the world in one transaction. The world holds two players, so that it's easy to fill. Once it's full, it turns players away, and the lobby puts them back on the waiting list.

The project has a directory for each module and a `spacetime.json` file:

```text
my-game/
├── spacetime.json
├── game_world/
└── lobby/
```

### Write the receiving reducer

In the `game_world` module, write `admit_player` as an ordinary reducer:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// game_world/src/index.ts
import { schema, table, t, SenderError } from 'spacetimedb/server';

const MAX_PLAYERS = 2n;

const player = table({ public: true }, { name: t.string().primaryKey() });

const spacetimedb = schema({ player });
export default spacetimedb;

export const admitPlayer = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (ctx.db.player.count() >= MAX_PLAYERS) {
      throw new SenderError('The world is full');
    }
    ctx.db.player.insert({ name });
  }
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// game_world/src/lib.rs
use spacetimedb::{reducer, table, ReducerContext, Table};

const MAX_PLAYERS: u64 = 2;

#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    name: String,
}

#[reducer]
pub fn admit_player(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if ctx.db.player().count() >= MAX_PLAYERS {
        return Err("The world is full".to_string());
    }
    ctx.db.player().insert(Player { name });
    Ok(())
}
```

</TabItem>
</Tabs>

`admit_player` turns a player away by failing with an error message. That error ends the call, and it's what the lobby receives.

### Generate the receiver bindings

The calling module needs **receiver bindings**: code generated from the receiving module, in the calling module's language, that describes the receiving module's reducers. To set them up, list the receiving database in the calling database's `dependencies` in `spacetime.json`:

```json
{
  "server": "local",
  "database": "lobby",
  "module-path": "./lobby",
  "dependencies": [{ "name": "game_world", "database": "game-world" }],
  "children": [{ "database": "game-world", "module-path": "./game_world" }]
}
```

The dependency's `name` names the generated bindings, and its `database` points at the entry that holds the receiving module's source. Then generate the bindings:

```bash
spacetime generate lobby
```

This writes the bindings for `game_world` to `lobby/src/remote_bindings/game_world`. Generate them again whenever the receiving module's reducers change. [`spacetime dev`](./00200-spacetime-dev.md#receiver-dependencies) regenerates them before each build. See [Receiver dependencies](../../00300-resources/00100-cli-reference/00300-spacetime-json.md#receiver-dependencies) for other ways to configure them.

### Declare an outbox and send calls

In the `lobby` module, declare the outbox table, a reducer that sends players to a world, and an `on_result` reducer that handles rejections:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// lobby/src/index.ts
import { schema, table, t, SenderError } from 'spacetimedb/server';
import gw from './remote_bindings/game_world';

const waitingPlayer = table(
  { public: true },
  { name: t.string().primaryKey() }
);

// Each row is a call to `admitPlayer` in a game world.
const admitPlayerOutbox = table(
  { outbox: { receiver: gw.reducers.admitPlayer } },
  {
    msgId: t.u64().primaryKey().autoInc(),
    world: gw.identity().target(),
    name: t.string(),
  }
);

const spacetimedb = schema({ waitingPlayer, admitPlayerOutbox });
export default spacetimedb;

export const joinLobby = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    ctx.db.waitingPlayer.insert({ name });
  }
);

export const sendToWorld = spacetimedb.reducer(
  { name: t.string(), world: gw.identity() },
  (ctx, { name, world }) => {
    if (!ctx.db.waitingPlayer.name.delete(name)) {
      throw new SenderError(`${name} is not in the lobby`);
    }
    // The call is sent if, and only if, this transaction commits.
    ctx.db.admitPlayerOutbox.insert({ msgId: 0n, world, name });
  }
);

export const onAdmitPlayerResult = spacetimedb.reducer(
  { onResult: admitPlayerOutbox },
  { row: admitPlayerOutbox.rowType, result: t.result(t.unit(), t.string()) },
  (ctx, { row, result }) => {
    if ('err' in result) {
      // The world turned the player away, so put them back in the lobby.
      console.warn(`${row.name} was not admitted: ${result.err}`);
      ctx.db.waitingPlayer.insert({ name: row.name });
    }
  }
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// lobby/src/lib.rs
mod remote_bindings;

use remote_bindings::game_world;
use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = waiting_player, public)]
pub struct WaitingPlayer {
    #[primary_key]
    name: String,
}

// Each row is a call to `admit_player` in a game world.
#[table(accessor = admit_player_outbox, outbox(game_world::admit_player))]
pub struct AdmitPlayerOutbox {
    #[primary_key]
    #[auto_inc]
    msg_id: u64,
    #[target]
    world: game_world::Identity,
    name: String,
}

#[reducer]
pub fn join_lobby(ctx: &ReducerContext, name: String) {
    ctx.db.waiting_player().insert(WaitingPlayer { name });
}

#[reducer]
pub fn send_to_world(ctx: &ReducerContext, name: String, world: game_world::Identity) -> Result<(), String> {
    if !ctx.db.waiting_player().name().delete(&name) {
        return Err(format!("{name} is not in the lobby"));
    }
    // The call is sent if, and only if, this transaction commits.
    ctx.db
        .admit_player_outbox()
        .insert(AdmitPlayerOutbox { msg_id: 0, world, name });
    Ok(())
}

#[reducer(on_result(admit_player_outbox))]
pub fn on_admit_player_result(ctx: &ReducerContext, row: AdmitPlayerOutbox, result: Result<(), String>) {
    if let Err(error) = result {
        // The world turned the player away, so put them back in the lobby.
        log::warn!("{} was not admitted: {error}", row.name);
        ctx.db.waiting_player().insert(WaitingPlayer { name: row.name });
    }
}
```

</TabItem>
</Tabs>

The rest of this page uses the names that SQL and the CLI use, such as `admit_player_outbox`. TypeScript code spells them in camel case, such as `admitPlayerOutbox`.

`admit_player_outbox` is the outbox table. Its outbox option names the reducer it calls, `admit_player` from the `game_world` bindings, and each of its rows is one call. `msg_id` identifies the row. `world` is the target column, which holds the identity of the database to call. `name` supplies the `name` parameter of `admit_player`. [Outbox tables](#outbox-tables) describes these rules in full.

`send_to_world` removes the player from `waiting_player` and inserts a row into the outbox, in the same transaction. If the player isn't waiting, the reducer fails, its transaction rolls back, and no call is sent.

`on_admit_player_result` is the outbox's `on_result` reducer. After `admit_player` has run in the world, SpacetimeDB runs `on_admit_player_result` in the lobby, passing it the row and the result of the call. If the world turned the player away, it puts them back on the waiting list. See [Handling results](#handling-results).

### Try it

With a local server running (`spacetime start`), publish both databases:

```bash
spacetime publish
```

The output includes each database's identity, which `spacetime list` also shows:

```text
Created new database with name: game-world, identity: c200a4f81288f09fafa9b3f785cf16732df3bea71a81cfaf3647e23cbc417e52
```

Add three players to the lobby, and send each of them to the world:

```bash
spacetime call lobby join_lobby alice
spacetime call lobby join_lobby bob
spacetime call lobby join_lobby carol
spacetime call lobby send_to_world alice {world_identity}
spacetime call lobby send_to_world bob {world_identity}
spacetime call lobby send_to_world carol {world_identity}
```

where _world_identity_ is the identity of `game-world`. For example:

```bash
spacetime call lobby send_to_world alice c200a4f81288f09fafa9b3f785cf16732df3bea71a81cfaf3647e23cbc417e52
```

The world admits the first two players:

```bash
spacetime sql game-world "SELECT * FROM player"
```

```text
 name
---------
 "alice"
 "bob"
```

It turns `carol` away, so the lobby's `on_result` reducer puts `carol` back on the waiting list:

```bash
spacetime sql lobby "SELECT * FROM waiting_player"
```

```text
 name
---------
 "carol"
```

The lobby's log, which `spacetime logs lobby` prints, says why: `carol was not admitted: The world is full`.

## Outbox tables

An outbox table's rows are calls to one reducer in another database. To declare one, give a table the outbox option with a reducer from the receiver bindings:

| | TypeScript | Rust |
| --- | --- | --- |
| Outbox option | `outbox: { receiver: gw.reducers.admitPlayer }` in the options of `table()` | `outbox(game_world::admit_player)` in `#[table(...)]` |
| Message ID column | `t.u64().primaryKey().autoInc()` | `#[primary_key]` and `#[auto_inc]` on a `u64` |
| Target column | `gw.identity().target()` | `#[target]` on a `game_world::Identity` |
| Parameter alias | `.param({ name: 'name' })` | `#[param(name = name)]` |

Its columns must follow these rules:

- **One message ID.** The primary key must be a single `u64` column with auto-increment. Insert rows with `0` in it, and SpacetimeDB assigns the ID. The column can have any name.
- **One target.** Exactly one column must hold the identity of the database to call. It must have the identity type from the receiving module's bindings, and be marked as the target. A plain `Identity` column can't be the target. The column can have any name.
- **One column for each parameter.** For each parameter of the receiving reducer, exactly one column must have the same name and type. The columns can be in any order. To give a column a different name, map it to its parameter with an alias. The message ID and target columns can supply parameters too.

The table can have other columns. SpacetimeDB doesn't send them, but they stay on the row, so the `on_result` reducer can read them. This version of the lobby's outbox records when each call was requested, and names its `name` column `player_name`:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const admitPlayerOutbox = table(
  { outbox: { receiver: gw.reducers.admitPlayer } },
  {
    msgId: t.u64().primaryKey().autoInc(),
    world: gw.identity().target(),
    // Supplies the `name` parameter of admitPlayer.
    playerName: t.string().param({ name: 'name' }),
    // Not sent. The onResult reducer can read it.
    requestedAt: t.timestamp(),
  }
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{table, Timestamp};

#[table(accessor = admit_player_outbox, outbox(game_world::admit_player))]
pub struct AdmitPlayerOutbox {
    #[primary_key]
    #[auto_inc]
    msg_id: u64,
    #[target]
    world: game_world::Identity,
    // Supplies the `name` parameter of admit_player.
    #[param(name = name)]
    player_name: String,
    // Not sent. The on_result reducer can read it.
    requested_at: Timestamp,
}
```

</TabItem>
</Tabs>

An outbox table can be public, and can have indexes and constraints like any other table. It can't also be a [schedule table](../00300-tables/00500-schedule-tables.md) or an [event table](../00300-tables/00550-event-tables.md).

Outbox rows are insert-only. Module code can insert and read them, but can't update or delete them. SpacetimeDB deletes each row once its call is finished: after the receiving database has run the call, and after the `on_result` reducer, if there is one, has run. Until then, the row shows that the call is pending, so you can list pending calls with SQL:

```bash
spacetime sql lobby "SELECT * FROM admit_player_outbox"
```

### Declaring an outbox in one line (TypeScript)

In TypeScript, the bindings can declare an outbox for you. `gw.outbox.admitPlayer()` returns an outbox table with a `msgId` message ID column, a `target` column, and a column for each parameter of the reducer:

```typescript
const spacetimedb = schema({
  waitingPlayer,
  admitPlayerOutbox: gw.outbox.admitPlayer(),
});

// Inside a reducer:
ctx.db.admitPlayerOutbox.insert({ msgId: 0n, target: world, name });
```

Like `table()`, it takes an optional `name`. Without one, the table is named after its key in `schema()`. It can't declare an outbox for a reducer with a parameter named `msgId` or `target`. For that reducer, use `table()`, and give the message ID and target columns other names.

## Receiving calls

The receiving reducer can be any reducer that clients can call, which excludes [lifecycle reducers](../00200-functions/00300-reducers/00500-lifecycle.md), scheduled reducers, and `on_result` reducers. It runs in its own transaction, like a call from a client, and `ctx.sender` is the calling database's identity. If it fails, its changes roll back, and the error goes back to the calling database.

A receiving reducer is still an ordinary reducer, so clients and any other database can call it too. To accept calls only from databases you trust, keep their identities in a table, and check `ctx.sender` against it. This version of the world's `admit_player` accepts calls only from trusted lobbies:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const trustedLobby = table({}, { identity: t.identity().primaryKey() });

const spacetimedb = schema({ player, trustedLobby });
export default spacetimedb;

export const admitPlayer = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (!ctx.db.trustedLobby.identity.find(ctx.sender)) {
      throw new SenderError('Only a trusted lobby can admit players');
    }
    ctx.db.player.insert({ name });
  }
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[table(accessor = trusted_lobby)]
pub struct TrustedLobby {
    #[primary_key]
    identity: Identity,
}

#[reducer]
pub fn admit_player(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if ctx.db.trusted_lobby().identity().find(ctx.sender()).is_none() {
        return Err("Only a trusted lobby can admit players".to_string());
    }
    ctx.db.player().insert(Player { name });
    Ok(())
}
```

</TabItem>
</Tabs>

Fill the table the way you'd manage any allow list, for example with a reducer that only an administrator can call. See [Admin and moderator roles](../../00250-how-to/01200-common-app-patterns/00200-admin-roles.md).

:::warning
SpacetimeDB runs the receiving database's [`client_connected`](../00200-functions/00300-reducers/00500-lifecycle.md#client-connected) and [`client_disconnected`](../00200-functions/00300-reducers/00500-lifecycle.md#client-disconnected) reducers around every call from another database, with `ctx.sender` set to the calling database and no JWT. If `client_connected` rejects the calling database, for example because it requires a JWT, the call isn't delivered, and the calling database keeps retrying it until `client_connected` accepts it. Likewise, a `client_connected` reducer that creates a row for each new identity creates one for each calling database.
:::

## Handling results

To learn how each call turned out, declare an `on_result` reducer for the outbox. It takes the outbox row and the result of the call:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const onAdmitPlayerResult = spacetimedb.reducer(
  { onResult: admitPlayerOutbox },
  { row: admitPlayerOutbox.rowType, result: t.result(t.unit(), t.string()) },
  (ctx, { row, result }) => {
    if ('err' in result) {
      // The call failed, and result.err says why.
    }
  }
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(on_result(admit_player_outbox))]
pub fn on_admit_player_result(ctx: &ReducerContext, row: AdmitPlayerOutbox, result: Result<(), String>) {
    if let Err(error) = result {
        // The call failed, and `error` says why.
    }
}
```

</TabItem>
</Tabs>

SpacetimeDB runs the `on_result` reducer in the calling database once for each row, in the same transaction that deletes the row from the outbox, so it never runs twice for one call and never misses one. An outbox can have at most one `on_result` reducer. Without one, the outbox is fire-and-forget: SpacetimeDB deletes each row once its call is finished, and nothing reports how it went.

Clients can't call an `on_result` reducer. Like a scheduled reducer, it's private, so only SpacetimeDB, the database owner, and team collaborators can run it.

The result depends on what happened in the receiving database:

| What happened | Result | Retried |
| --- | --- | --- |
| The receiving reducer succeeded. | Success | No |
| The receiving reducer returned or threw an error. | The error's message | No |
| The receiving reducer panicked or ran out of energy. | An error describing the failure | No |
| The receiving database doesn't have the reducer, or the reducer's parameters don't match the bindings. | An error describing the mismatch | No |
| The receiving database couldn't be reached, or its `client_connected` reducer rejected the call. | None yet | Yes, until one of the outcomes above happens |

SpacetimeDB never retries a call once it has a result. Calls through one outbox to one database run in order, so retrying a call that fails every time would hold up every call behind it. To try again, insert a new row, for example from the `on_result` reducer.

If the `on_result` reducer itself fails, its changes roll back, but SpacetimeDB still deletes the row, and doesn't run the `on_result` reducer for that row again.

## Target identities

A target column's type comes from the receiving module's bindings: `gw.identity()` in TypeScript, and `game_world::Identity` in Rust. Each set of bindings has its own identity type, so the compiler rejects a row that sends one module's reducer to an identity meant for another module. In the database and to clients, the column is an ordinary `Identity`.

The type records which module you mean to call. It doesn't check that the identity belongs to a database running that module. A call to a database that doesn't have a matching reducer fails with an error. A call to an identity that no database has is never delivered, and SpacetimeDB keeps retrying it.

A reducer can take a target as an argument, as `send_to_world` does, or read one from a table column of the same type. In Rust, wrap a plain `Identity` to get one: `game_world::Identity(identity)`. In TypeScript, a plain `Identity` can't be a target, but index lookups on a column of the target's type accept one. To call back the database that called you, look up `ctx.sender` in a table of databases you trust, and use the identity in the row that the lookup returns.

## Changing a receiving reducer

Receiver bindings capture the receiving reducer's name and parameters when you generate them, and each call carries a hash of that signature. The receiving database checks the hash before running the reducer. After you add, remove, rename, or reorder a parameter, or change its type, calls built from older bindings fail without running, including calls that were already waiting in an outbox. The calling database's `on_result` reducer receives an error like this one:

```text
IDC reducer signature mismatch for admit_player: expected 085a98d187205ddd5cb574208fc8f7aa6728c078ae72865f742a48751056824d, got c9d10b4c604f6cb0f1a51826e6b23b0779772ff375d196fd26b0d5754af9b56a
```

To pick up the change, regenerate the bindings and republish the calling database. If the outbox no longer matches the reducer, the calling module fails to compile, with an error that names the parameter:

```text
error: outbox has no column matching remote reducer parameter `level`
```

Adding a column to an outbox table follows the usual [automatic migration](./00500-migrations/00200-automatic-migrations.md) rules, so the new column needs a default value.

To change a reducer without failing calls from databases that haven't regenerated their bindings, add a new reducer beside the old one, and remove the old one once every calling database has moved to the new one. Renaming the function that implements a reducer doesn't affect callers, as long as the reducer's name in the database stays the same.

## Current limitations

- Only Rust and TypeScript modules can declare outbox tables.
- The calling and receiving databases must be published to the same server. A database can't call a database on another server, such as a database on Maincloud from a self-hosted one.
- Receiver bindings are generated from the receiving module's source code, so the calling project needs a copy of it. They can't be generated from a published database alone.
- Receiver bindings include only the receiving module's own reducers, not the reducers of its [submodules](./00600-submodules.md).
- Outbox rows can't be updated or deleted, so a pending call can't be cancelled.
- A call that can't be delivered, for example because no database has the target identity, stays pending and holds up later calls through the same outbox to the same database.
- In Rust, an index on a column of a receiver's identity type, such as `game_world::Identity`, can't be used for lookups. Iterate over the table and compare instead.

## Next steps

- See [Receiver dependencies](../../00300-resources/00100-cli-reference/00300-spacetime-json.md#receiver-dependencies) for every way to configure receiver bindings in `spacetime.json`.
- Learn how [transactions](./00100-transactions-atomicity.md) make each reducer's changes, including its outbox rows, all-or-nothing.
- Use a [schedule table](../00300-tables/00500-schedule-tables.md) to call a reducer in the same database at a later time.
