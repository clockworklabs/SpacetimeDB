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

Schema builders and module exports come from `spacetimedb/server`. Runtime value classes such as `ScheduleAt`, `Timestamp`, `Range`, and `ConnectionId` come from the root `spacetimedb` package:

```typescript
import { schema, table, t } from 'spacetimedb/server';
import { SenderError } from 'spacetimedb/server';
import { ConnectionId, ScheduleAt, Timestamp } from 'spacetimedb';
import { Range } from 'spacetimedb/server';
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
| `t.u8()` / `t.u16()` / `t.u32()` | number | |
| `t.i8()` / `t.i16()` / `t.i32()` | number | |
| `t.u64()` | bigint | Use `0n` literals |
| `t.i64()` | bigint | Use `0n` literals |
| `t.u128()` / `t.i128()` / `t.u256()` / `t.i256()` | bigint | |
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

`ctx.connectionId` is `ConnectionId | null`, including in lifecycle contexts. Guard it before passing it to a helper or using it as a table key.

## Reducer Context API

`ctx` is the only source of sender identity, time, and randomness; stdlib clocks and RNG are unavailable in modules. In helpers, type it as `ReducerCtx<InferSchema<typeof spacetimedb>>`.

```typescript
// Auth: ctx.sender is the caller's Identity
if (!row.owner.equals(ctx.sender)) throw new SenderError('unauthorized');

// Server timestamp (deterministic per reducer call)
ctx.db.item.insert({ id: 0n, createdAt: ctx.timestamp });

// Deterministic RNG
const f: number = ctx.random();                          // [0.0, 1.0)
const roll: number = ctx.random.integerInRange(1, 6);    // safe JS number bounds/result, inclusive
const storedRoll: bigint = BigInt(roll);                 // convert the result for an i64/u64 column
const bytes: Uint8Array = ctx.random.fill(new Uint8Array(16));

// Client: Timestamp → Date
new Date(Number(row.createdAt.microsSinceUnixEpoch / 1000n));
```

Do not construct `Identity` values from strings (e.g. `'hex' as Identity`): serialization fails and kills the module. Identities come from `ctx.sender` or `t.identity()` columns.

Synthetic connection IDs for module logic/tests can use `new ConnectionId(1n)` after importing `ConnectionId` from `spacetimedb`.

Construct exact timestamps with `new Timestamp(micros)` after importing `Timestamp` from `spacetimedb`. Inclusive index ranges use `Range` from `spacetimedb/server`:

```typescript
import { Timestamp } from 'spacetimedb';
import { Range } from 'spacetimedb/server';

ctx.db.shipment.deliverBy.filter(new Range(
  { tag: 'included', value: new Timestamp(1_000n) },
  { tag: 'included', value: new Timestamp(2_000n) },
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

`t.row(...)` and `t.object(...)` return schema builders, not TypeScript runtime row types. Let a view callback infer its result, or annotate a separately declared structural type such as `Array<{ sku: bigint; label: string }>`. A named output type must not reuse the generated PascalCase name of its view accessor (for example, reserve `DiscountedProduct` for a `discounted_product` view).

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
const CatalogKey = t.row('CatalogKey', {
  sku: t.u64().primaryKey(),
  label: t.string(),
});
```

Query-builder views use `ctx.from` and return the query directly:

```typescript
export const discountedProduct = spacetimedb.view(
  { name: 'discounted_product', public: true },
  t.array(product.rowType),
  ctx => ctx.from.product.where(product => product.discounted.eq(true))
);

export const taggedProduct = spacetimedb.view(
  { name: 'tagged_product', public: true },
  t.array(product.rowType),
  ctx => ctx.from.productTag.rightSemijoin(
    ctx.from.product,
    (tag, product) => tag.productId.eq(product.id)
  )
);
```

## Client Visibility Filters

```typescript
export const privateNoteFilter = spacetimedb.clientVisibilityFilter.sql(
  'SELECT * FROM private_note WHERE author = :sender'
);
```

## Procedures and HTTP

Procedures declare argument and return types. They can perform outbound HTTP through `ctx.http` and open short transactions with `ctx.withTx`:

```typescript
const Product = t.object('Product', { value: t.u32(), description: t.string() });

export const multiply = spacetimedb.procedure(
  { lhs: t.u32(), rhs: t.u32() },
  Product,
  (_ctx, { lhs, rhs }) => ({ value: lhs * rhs, description: 'product' })
);

export const refreshCache = spacetimedb.procedure(
  { url: t.string() },
  t.unit(),
  (ctx, { url }) => {
    const response = ctx.http.fetch(url);
    ctx.withTx(tx => tx.db.cacheEntry.insert({ key: url, status: response.status }));
    return {};
  }
);
```

TypeScript outbound HTTP uses `ctx.http.fetch(url, options)`, including for non-GET requests; it does not provide convenience methods such as `get()` or `post()`. Responses expose the numeric `status`, `headers.get(name)`, and `text()` APIs. Perform network I/O before `withTx`; only database work belongs inside its callback.

Scheduled procedures use a normal scheduled table, but its `scheduled` option references a `spacetimedb.procedure(...)` export instead of a reducer. The procedure receives its scheduled row as an argument and uses `withTx` for database access:

```typescript
const cleanup_timer = table(
  { name: 'cleanup_timer', scheduled: (): any => runCleanup },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    cacheKey: t.string(),
  }
);

export const runCleanup = spacetimedb.procedure(
  { timer: cleanup_timer.rowType },
  t.unit(),
  (ctx, { timer }) => {
    ctx.withTx(tx => tx.db.cacheEntry.key.delete(timer.cacheKey));
    return {};
  }
);
```

Inbound HTTP uses `httpHandler`, `httpRouter`, `Router`, and `SyncResponse`:

```typescript
import { Router, SyncResponse } from 'spacetimedb/server';

export const health = spacetimedb.httpHandler((_ctx, _request) =>
  new SyncResponse('ok', {
    status: 200,
    headers: { 'content-type': 'text/plain' },
  })
);
export const routes = spacetimedb.httpRouter(new Router().get('/health', health));
```

Handlers are synchronous: return `SyncResponse` directly rather than marking the callback `async`. Pass the exported `httpHandler(...)` value to the router, not its raw callback. The router selects the path, while a handler reads request data with APIs such as `request.text()`; `Request` has no `path` property. A handler context does not expose `ctx.db`; use `ctx.withTx(tx => ...)` when a handler needs transactional database access.
