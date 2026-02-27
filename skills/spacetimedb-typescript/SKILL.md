---
name: spacetimedb-typescript
description: Build TypeScript clients for SpacetimeDB. Use when connecting to SpacetimeDB from web apps, Node.js, Deno, Bun, or other JavaScript runtimes.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
---

# SpacetimeDB TypeScript SDK

Build real-time TypeScript clients that connect directly to SpacetimeDB modules. The SDK provides type-safe database access, automatic synchronization, and reactive updates for web apps, Node.js, Deno, Bun, and other JavaScript runtimes.

---

## HALLUCINATED APIs — DO NOT USE

**These APIs DO NOT EXIST. LLMs frequently hallucinate them.**

```typescript
// WRONG PACKAGE — does not exist
import { SpacetimeDBClient } from "@clockworklabs/spacetimedb-sdk";

// WRONG — these methods don't exist
SpacetimeDBClient.connect(...);
SpacetimeDBClient.call("reducer_name", [...]);
connection.call("reducer_name", [arg1, arg2]);

// WRONG — positional reducer arguments
conn.reducers.doSomething("value");  // WRONG!

// WRONG — old 1.0 patterns
spacetimedb.reducer('reducer_name', params, fn);  // Use export const name = spacetimedb.reducer(params, fn)
schema(myTable);          // Use schema({ myTable })
schema(t1, t2, t3);      // Use schema({ t1, t2, t3 })
scheduled: 'run_cleanup'  // Use scheduled: () => run_cleanup
.withModuleName('db')     // Use .withDatabaseName('db') (2.0)
.withLightMode(true)      // Removed in 2.0
setReducerFlags.x('NoSuccessNotify')  // Removed in 2.0
```

### CORRECT PATTERNS:

```typescript
// CORRECT IMPORTS
import { DbConnection, tables } from './module_bindings';  // Generated!
import { SpacetimeDBProvider, useTable, Identity } from 'spacetimedb/react';

// CORRECT REDUCER CALLS — object syntax, not positional!
conn.reducers.doSomething({ value: 'test' });
conn.reducers.updateItem({ itemId: 1n, newValue: 42 });

// CORRECT DATA ACCESS — useTable returns [rows, isLoading]
const [items, isLoading] = useTable(tables.item);
```

### DO NOT:
- **Invent hooks** like `useItems()`, `useData()` — use `useTable(tables.tableName)`
- **Import from fake packages** — only `spacetimedb`, `spacetimedb/react`, `./module_bindings`

---

## Common Mistakes Table

### Server-side errors

| Wrong | Right | Error |
|-------|-------|-------|
| Missing `package.json` | Create `package.json` | "could not detect language" |
| Missing `tsconfig.json` | Create `tsconfig.json` | "TsconfigNotFound" |
| Entrypoint not at `src/index.ts` | Use `src/index.ts` | Module won't bundle |
| `indexes` in COLUMNS (2nd arg) | `indexes` in OPTIONS (1st arg) | "reading 'tag'" error |
| Index without `algorithm` | `algorithm: 'btree'` | "reading 'tag'" error |
| `filter({ ownerId })` | `filter(ownerId)` | "does not exist in type 'Range'" |
| `.filter()` on unique column | `.find()` on unique column | TypeError |
| `insert({ ...without id })` | `insert({ id: 0n, ... })` | "Property 'id' is missing" |
| `const id = table.insert(...)` | `const row = table.insert(...)` | `.insert()` returns ROW, not ID |
| `.unique()` + explicit index | Just use `.unique()` | "name is used for multiple entities" |
| Import spacetimedb from index.ts | Import from schema.ts | "Cannot access before initialization" |
| Multi-column index `.filter()` | Use single-column index | PANIC or silent empty results |
| `.iter()` in views | Use index lookups only | Views can't scan tables |
| `ctx.db` in procedures | `ctx.withTx(tx => tx.db...)` | Procedures need explicit transactions |
| `reducer('name', params, fn)` | `export const name = spacetimedb.reducer(params, fn)` | 2.0: name from export |
| `schema(myTable)` | `schema({ myTable })` | 2.0: object argument only |

### Client-side errors

| Wrong | Right | Error |
|-------|-------|-------|
| `@spacetimedb/sdk` | `spacetimedb` | 404 / missing subpath |
| `conn.reducers.foo("val")` | `conn.reducers.foo({ param: "val" })` | Wrong reducer syntax |
| Inline `connectionBuilder` | `useMemo(() => ..., [])` | Reconnects every render |
| `const rows = useTable(table)` | `const [rows, isLoading] = useTable(table)` | Tuple destructuring |
| Optimistic UI updates | Let subscriptions drive state | Desync issues |
| `<SpacetimeDBProvider builder={...}>` | `connectionBuilder={...}` | Wrong prop name |
| `.withModuleName()` | `.withDatabaseName()` | 2.0 renamed method |

---

## Hard Requirements

1. **`schema({ table })`** — takes exactly one object; never `schema(table)` or `schema(t1, t2, t3)`
2. **Reducer/procedure names from exports** — `export const name = spacetimedb.reducer(params, fn)`; never `reducer('name', ...)`
3. **Reducer calls use object syntax** — `{ param: 'value' }` not positional args
4. **Import `DbConnection` from `./module_bindings`** — not from `spacetimedb`
5. **DO NOT edit generated bindings** — regenerate with `spacetime generate`
6. **Indexes go in OPTIONS (1st arg)** — not in COLUMNS (2nd arg) of `table()`
7. **Use BigInt for u64/i64 fields** — `0n`, `1n`, not `0`, `1`
8. **Reducers are transactional** — they do not return data
9. **Reducers must be deterministic** — no filesystem, network, timers, random
10. **Views should use index lookups** — `.iter()` causes severe performance issues
11. **Procedures need `ctx.withTx()`** — `ctx.db` doesn't exist in procedures
12. **Sum type values** — use `{ tag: 'variant', value: payload }` not `{ variant: payload }`
13. **Use `.withDatabaseName()`** — not `.withModuleName()` (2.0)

---

## Installation

```bash
npm install spacetimedb
```

For Node.js 18-21, also install `undici`. Node.js 22+ and browsers work out of the box.

## Generating Type Bindings

```bash
spacetime generate --lang typescript --out-dir ./src/module_bindings --module-path ./server
```

## Basic Connection Setup

```typescript
import { DbConnection } from './module_bindings';

const connection = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my_database')
  .withToken(localStorage.getItem('spacetimedb_token') ?? undefined)
  .onConnect((conn, identity, token) => {
    localStorage.setItem('spacetimedb_token', token);
    conn.subscriptionBuilder().subscribe('SELECT * FROM player');
  })
  .onDisconnect((ctx) => console.log('Disconnected'))
  .onConnectError((ctx, error) => console.error('Error:', error))
  .build();
```

## Subscribing to Tables

```typescript
// Basic subscription
connection.subscriptionBuilder()
  .onApplied((ctx) => console.log('Cache ready'))
  .subscribe('SELECT * FROM player');

// Multiple queries
connection.subscriptionBuilder()
  .subscribe(['SELECT * FROM player', 'SELECT * FROM game_state']);

// Subscribe to all tables (development only)
connection.subscriptionBuilder().subscribeToAllTables();
```

## Accessing Table Data

```typescript
for (const player of connection.db.player.iter()) { console.log(player.name); }
const players = Array.from(connection.db.player.iter());
const count = connection.db.player.count();
const player = connection.db.player.id.find(42);
```

## Table Event Callbacks

```typescript
connection.db.player.onInsert((ctx, player) => console.log('New:', player.name));
connection.db.player.onDelete((ctx, player) => console.log('Left:', player.name));
connection.db.player.onUpdate((ctx, old, new_) => console.log(`${old.score} -> ${new_.score}`));
```

## Calling Reducers

**CRITICAL: Use object syntax, not positional arguments.**

```typescript
connection.reducers.createPlayer({ name: 'Alice', location: { x: 0, y: 0 } });

connection.reducers.onCreatePlayer((ctx, args) => {
  if (ctx.event.status.tag === 'Committed') console.log('Success');
  else if (ctx.event.status.tag === 'Failed') console.error(ctx.event.status.value);
});
```

### Snake_case to camelCase conversion
- Server: `export const do_something = spacetimedb.reducer(...)`
- Client: `conn.reducers.doSomething({ ... })`

---

## Server-Side Module Development

### Table Definition

```typescript
import { schema, table, t } from 'spacetimedb/server';

export const Task = table({
  name: 'task',
  public: true,
  indexes: [{ name: 'task_owner_id', algorithm: 'btree', columns: ['ownerId'] }]
}, {
  id: t.u64().primaryKey().autoInc(),
  ownerId: t.identity(),
  title: t.string(),
  createdAt: t.timestamp(),
});
```

### Column types

```typescript
t.identity()           // User identity
t.u64()                // Unsigned 64-bit integer (use for IDs)
t.string()             // Text
t.bool()               // Boolean
t.timestamp()          // Timestamp
t.scheduleAt()         // For scheduled tables only
t.object('Name', {})   // Product types (nested objects)
t.enum('Name', {})     // Sum types (tagged unions)
t.string().optional()  // Nullable
```

> BigInt syntax: All `u64`/`i64` fields use `0n`, `1n`, not `0`, `1`.

### Schema export

```typescript
const spacetimedb = schema({ Task, Player });
export default spacetimedb;
```

### Reducer Definition (2.0)

**Name comes from the export — NOT from a string argument.**

```typescript
import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';

export const create_task = spacetimedb.reducer(
  { title: t.string() },
  (ctx, { title }) => {
    if (!title) throw new SenderError('title required');
    ctx.db.task.insert({ id: 0n, ownerId: ctx.sender, title, createdAt: ctx.timestamp });
  }
);
```

### Update Pattern

```typescript
const existing = ctx.db.task.id.find(taskId);
if (!existing) throw new SenderError('Task not found');
ctx.db.task.id.update({ ...existing, title: newTitle, updatedAt: ctx.timestamp });
```

### Lifecycle Hooks

```typescript
spacetimedb.clientConnected((ctx) => { /* ctx.sender is the connecting identity */ });
spacetimedb.clientDisconnected((ctx) => { /* clean up */ });
```

---

## Event Tables (2.0)

Reducer callbacks are removed in 2.0. Use event tables + `onInsert` instead.

```typescript
export const DamageEvent = table(
  { name: 'damage_event', public: true, event: true },
  { target: t.identity(), amount: t.u32() }
);

export const deal_damage = spacetimedb.reducer(
  { target: t.identity(), amount: t.u32() },
  (ctx, { target, amount }) => {
    ctx.db.damageEvent.insert({ target, amount });
  }
);
```

Client subscribes and uses `onInsert`:
```typescript
conn.db.damageEvent.onInsert((ctx, evt) => {
  playDamageAnimation(evt.target, evt.amount);
});
```

Event tables must be subscribed explicitly — they are excluded from `subscribeToAllTables()`.

---

## Views

### ViewContext vs AnonymousViewContext

```typescript
// ViewContext — has ctx.sender, result varies per user
spacetimedb.view({ name: 'my_items', public: true }, t.array(Item.rowType), (ctx) => {
  return [...ctx.db.item.by_owner.filter(ctx.sender)];
});

// AnonymousViewContext — no ctx.sender, same result for everyone (better perf)
spacetimedb.anonymousView({ name: 'leaderboard', public: true }, t.array(Player.rowType), (ctx) => {
  return ctx.from.player.where(p => p.score.gt(1000));
});
```

Views can only use index lookups — `.iter()` is NOT allowed.

---

## Scheduled Tables

```typescript
export const CleanupJob = table({
  name: 'cleanup_job',
  scheduled: () => run_cleanup  // function returning the exported reducer
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  targetId: t.u64(),
});

export const run_cleanup = spacetimedb.reducer(
  { arg: CleanupJob.rowType },
  (ctx, { arg }) => { /* arg.scheduledId, arg.targetId available */ }
);

// Schedule a job
import { ScheduleAt } from 'spacetimedb';
ctx.db.cleanupJob.insert({
  scheduledId: 0n,
  scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + 60_000_000n),
  targetId: someId
});
```

---

## Timestamps

### Server-side
```typescript
ctx.db.item.insert({ id: 0n, createdAt: ctx.timestamp });
const future = ctx.timestamp.microsSinceUnixEpoch + 300_000_000n;
```

### Client-side (CRITICAL)
```typescript
// Timestamps are objects, not numbers
const date = new Date(Number(row.createdAt.microsSinceUnixEpoch / 1000n));
```

---

## Procedures (Beta)

```typescript
export const fetch_data = spacetimedb.procedure(
  { url: t.string() }, t.string(),
  (ctx, { url }) => {
    const response = ctx.http.fetch(url);
    ctx.withTx(tx => { tx.db.myTable.insert({ id: 0n, content: response.text() }); });
    return response.text();
  }
);
```

Procedures don't have `ctx.db` — use `ctx.withTx(tx => tx.db...)`.

---

## React Integration

```tsx
import { useMemo } from 'react';
import { SpacetimeDBProvider, useTable, useReducer } from 'spacetimedb/react';
import { DbConnection, tables, reducers, query } from './module_bindings';

function Root() {
  const connectionBuilder = useMemo(() =>
    DbConnection.builder()
      .withUri('ws://localhost:3000')
      .withDatabaseName('my_game')
      .withToken(localStorage.getItem('auth_token') || undefined)
      .onConnect((conn, identity, token) => {
        localStorage.setItem('auth_token', token);
        conn.subscriptionBuilder().subscribe(query.player.build());
      }),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  );
}

function PlayerList() {
  const [players, isLoading] = useTable(tables.player);
  if (isLoading) return <div>Loading...</div>;
  return <ul>{players.map(p => <li key={p.id}>{p.name}</li>)}</ul>;
}
```

---

## Project Structure

### Server (`backend/spacetimedb/`)
```
src/schema.ts   -> Tables, export spacetimedb
src/index.ts    -> Reducers, lifecycle, import schema
package.json    -> { "type": "module", "dependencies": { "spacetimedb": "^1.11.0" } }
tsconfig.json   -> Standard config
```

### Client (`client/`)
```
src/module_bindings/ -> Generated (spacetime generate)
src/main.tsx         -> Provider, connection setup
src/App.tsx          -> UI components
```

---

## Commands

```bash
spacetime start
spacetime publish <module-name> --module-path <backend-dir>
spacetime publish <module-name> --clear-database -y --module-path <backend-dir>
spacetime generate --lang typescript --out-dir <client>/src/module_bindings --module-path <backend-dir>
spacetime logs <module-name>
```
