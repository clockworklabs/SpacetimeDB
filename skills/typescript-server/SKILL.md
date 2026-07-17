---
name: typescript-server
description: SpacetimeDB TypeScript server module SDK reference. Use when writing tables, reducers, or module logic in TypeScript.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: server
  language: typescript
  cursor_globs: "**/*.ts"
  cursor_always_apply: true
---

# SpacetimeDB TypeScript SDK Reference

## Module Structure

Tables are built with `table()`, bound with `schema()`, and exported as default. Reducers and lifecycle hooks are `export const`:

```typescript
import { schema, table, t } from 'spacetimedb/server';

const score_record = table(
  { name: 'score_record', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.identity(),
    value: t.u32(),
  }
);

const spacetimedb = schema({ score_record });  // ONE object, not spread args
export default spacetimedb;

export const addRecord = spacetimedb.reducer(
  { value: t.u32() },
  (ctx, { value }) => {
    ctx.db.score_record.insert({ id: 0n, owner: ctx.sender, value });
  }
);
```

## Imports

`spacetimedb/server` is the only import path for server modules:

```typescript
import { schema, table, t } from 'spacetimedb/server';
import { SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';        // for scheduled tables only
```

## Tables

`table(OPTIONS, COLUMNS)` takes two arguments. The `name` field MUST be snake_case:

```typescript
const entity = table(
  { name: 'entity', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    active: t.bool(),
  }
);
```

Options: `name` (snake_case, recommended), `public: true`, `event: true`, `scheduled: (): any => reducerRef`, `indexes: [...]`

`ctx.db` accessors are the keys passed to `schema({...})`, verbatim: `schema({ score_record })` → `ctx.db.score_record`. Use snake_case keys matching the table `name`. Client codegen converts case; server `ctx.db` does not.

## Column Types

Every column is a `t` builder value:

| Builder | JS type | Notes |
|---------|---------|-------|
| `t.u64()` | bigint | Use `0n` literals |
| `t.i64()` | bigint | Use `0n` literals |
| `t.u32()` / `t.i32()` | number | |
| `t.f64()` / `t.f32()` | number | |
| `t.bool()` | boolean | |
| `t.string()` | string | |
| `t.identity()` | Identity | |
| `t.connectionId()` | ConnectionId | |
| `t.timestamp()` | Timestamp | |
| `t.timeDuration()` | TimeDuration | |
| `t.scheduleAt()` | ScheduleAt | |

Modifiers: `.primaryKey()`, `.autoInc()`, `.unique()`, `.index('btree')`, `.default(value)`.

Use `.default(value)` only for a newly appended migration-safe field. Preserve existing fields and reducers exactly, and do not put defaults on primary-key, unique, or auto-increment columns.

Additional numeric builders include `t.u128()`, `t.i128()`, `t.u256()`, and `t.i256()`.

Optional columns: `nickname: t.option(t.string())`

## Indexes

Prefer inline `.index('btree')` for single-column. Use named indexes only for multi-column:

```typescript
// Inline (preferred for single-column):
authorId: t.u64().index('btree'),
// Access: ctx.db.post.authorId.filter(authorId);

// Multi-column (named):
indexes: [{ accessor: 'by_group_user', algorithm: 'btree', columns: ['groupId', 'userId'] }]
// Access: ctx.db.membership.by_group_user.filter([groupId, userId]);
```

Prefer a multi-column index over filtering by one column and looping. Filter takes an array in index column order; a prefix scan passes the leading value bare: `filter(groupId)`.

## Reducers

Reducers are created with `spacetimedb.reducer(...)`; the export name becomes the reducer name:

```typescript
export const createEntity = spacetimedb.reducer(
  { name: t.string(), age: t.i32() },
  (ctx, { name, age }) => {
    ctx.db.entity.insert({ identity: ctx.sender, name, age, active: true });
  }
);

// No arguments, just the callback:
export const doReset = spacetimedb.reducer((ctx) => { ... });
```

Reducer args accept any column type, including arrays of custom types: `{ splits: t.array(Split) }`. Do not pass JSON strings for structured data.

## DB Operations

```typescript
ctx.db.score_record.insert({ id: 0n, owner: ctx.sender, value: 1 });  // Insert (0n for autoInc)
ctx.db.score_record.id.find(recordId);                     // Find by PK → row | null
ctx.db.entity.identity.find(ctx.sender);                   // Find by unique column
[...ctx.db.post.authorId.filter(authorId)];                // Filter → spread to Array
[...ctx.db.entity.iter()];                                 // All rows → Array
ctx.db.score_record.id.update({ ...existing, value: 2 });  // Update (spread + override)
ctx.db.score_record.id.delete(recordId);                   // Delete by PK
```

Note: `iter()` and `filter()` return iterators. Spread to Array for `.sort()`, `.filter()`, `.map()`.

## Lifecycle Hooks

MUST be `export const`. Bare calls are silently ignored:

```typescript
export const init = spacetimedb.init((ctx) => { ... });
export const onConnect = spacetimedb.clientConnected((ctx) => { ... });
export const onDisconnect = spacetimedb.clientDisconnected((ctx) => { ... });
```

## Reducer Context API

`ctx` is the only source of sender identity, time, and randomness; stdlib clocks and RNG are unavailable in modules. In helpers, type it as `ReducerCtx<InferSchema<typeof spacetimedb>>`.

```typescript
// Auth: ctx.sender is the caller's Identity
if (!row.owner.equals(ctx.sender)) throw new SenderError('unauthorized');

// Server timestamp (deterministic per reducer call)
ctx.db.item.insert({ id: 0n, createdAt: ctx.timestamp });

// Deterministic RNG
const f: number = ctx.random();                          // [0.0, 1.0)
const roll: number = ctx.random.integerInRange(1, 6);    // inclusive
const bytes: Uint8Array = ctx.random.fill(new Uint8Array(16));

// Client: Timestamp → Date
new Date(Number(row.createdAt.microsSinceUnixEpoch / 1000n));
```

Do not construct `Identity` values from strings (e.g. `'hex' as Identity`): serialization fails and kills the module. Identities come from `ctx.sender` or `t.identity()` columns.

Synthetic connection IDs for module logic/tests can use `new ConnectionId(1n)` after importing `ConnectionId` from `spacetimedb`.

Construct exact timestamps with `new Timestamp(micros)` after importing `Timestamp` from `spacetimedb`. Inclusive index ranges use `Range`:

```typescript
import { Range, Timestamp } from 'spacetimedb';

ctx.db.event.occurredAt.filter(new Range(
  { tag: 'included', value: new Timestamp(200n) },
  { tag: 'included', value: new Timestamp(400n) },
));
```

## Scheduled Tables

```typescript
import { ScheduleAt } from 'spacetimedb';   // ScheduleAt comes from the root package

const tick_timer = table({
  name: 'tick_timer',
  scheduled: (): any => tick,   // (): any => breaks circular dep
}, {
  scheduled_id: t.u64().primaryKey().autoInc(),
  scheduled_at: t.scheduleAt(),
});

export const tick = spacetimedb.reducer(
  { timer: tick_timer.rowType },
  (ctx, { timer }) => { /* timer row auto-deleted after this runs */ }
);

// One-time: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + delayMicros)
// Repeating: ScheduleAt.interval(60_000_000n)
```

## Custom Types

```typescript
// Product type (struct):
const Position = t.object('Position', { x: t.i32(), y: t.i32() });
const entity = table({ name: 'entity' }, {
  id: t.u64().primaryKey().autoInc(),
  pos: Position,
});

// Sum type (tagged union):
const Shape = t.enum('Shape', {
  circle: t.i32(),
  rectangle: t.object('Rect', { w: t.i32(), h: t.i32() }),
});
// Values: { tag: 'circle', value: 10 }
```

## Views

```typescript
// Anonymous view (same for all clients):
export const activeUsers = spacetimedb.anonymousView(
  { name: 'active_users', public: true },
  t.array(entity.rowType),
  (ctx) => [...ctx.db.entity.iter()].filter(e => e.active)
);

// Per-user view (varies by ctx.sender):
export const myProfile = spacetimedb.view(
  { name: 'my_profile', public: true },
  t.option(entity.rowType),
  (ctx) => ctx.db.entity.identity.find(ctx.sender) ?? undefined
);
```

For a procedural view primary key, define the output with `t.row` and mark its field `.primaryKey()`:

```typescript
const SourceViewRow = t.row('SourceViewRow', {
  id: t.u64().primaryKey(),
  value: t.string(),
});
```

Query-builder views use `ctx.from` and return the query directly:

```typescript
export const openTicket = spacetimedb.view(
  { name: 'open_ticket', public: true },
  t.array(ticket.rowType),
  ctx => ctx.from.ticket.where(ticket => ticket.status.eq('open'))
);

export const eligibleMember = spacetimedb.view(
  { name: 'eligible_member', public: true },
  t.array(member.rowType),
  ctx => ctx.from.eligibility.rightSemijoin(
    ctx.from.member,
    (eligibility, member) => eligibility.memberId.eq(member.id)
  )
);
```

## Client Visibility Filters

```typescript
export const userRecordFilter = spacetimedb.clientVisibilityFilter.sql(
  'SELECT * FROM user_record WHERE identity = :sender'
);
```

## Procedures and HTTP

Procedures declare argument and return types. They can perform outbound HTTP through `ctx.http` and open short transactions with `ctx.withTx`:

```typescript
const Summary = t.object('Summary', { total: t.u32(), label: t.string() });

export const calculateSummary = spacetimedb.procedure(
  { lhs: t.u32(), rhs: t.u32() },
  Summary,
  (_ctx, { lhs, rhs }) => ({ total: lhs + rhs, label: 'calculated' })
);

export const fetchAndStore = spacetimedb.procedure(
  { url: t.string() },
  t.unit(),
  (ctx, { url }) => {
    const response = ctx.http.fetch(url);
    ctx.withTx(tx => tx.db.fetchedRecord.insert({ id: 1n, status: response.status }));
    return {};
  }
);
```

Scheduled procedures use a normal scheduled table, but reference a `spacetimedb.procedure(...)` callback instead of a reducer.

Inbound HTTP uses `httpHandler`, `httpRouter`, `Router`, and `SyncResponse`:

```typescript
import { Router, SyncResponse } from 'spacetimedb/server';

export const echo = spacetimedb.httpHandler((_ctx, request) =>
  new SyncResponse(`echo:${request.text()}`, {
    status: 201,
    headers: { 'content-type': 'text/plain' },
  })
);
export const routes = spacetimedb.httpRouter(new Router().post('/echo', echo));
```
