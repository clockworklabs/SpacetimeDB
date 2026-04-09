# SpacetimeDB TypeScript Module Guidelines

## Imports

```typescript
import { schema, table, t } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';        // for scheduled tables
```

⚠️ CRITICAL: The `name` field in table() MUST be snake_case (e.g. 'order_detail', NOT 'orderDetail').
This is the single most common mistake. The JS variable can be camelCase, the `name` string cannot.

## Tables

`table(OPTIONS, COLUMNS)` — two arguments:

```typescript
const user = table(
  { name: 'user', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    email: t.string(),
    name: t.string(),
    active: t.bool(),
  }
);
```

**IMPORTANT: The `name` string MUST be snake_case** — it becomes the SQL table name.
The JS variable can be camelCase, but the `name` string is always snake_case:

```typescript
const orderDetail = table({ name: 'order_detail' }, { ... });  // ✓ snake_case name
const userStats = table({ name: 'user_stats' }, { ... });      // ✓ snake_case name
const eventLog = table({ name: 'event_log' }, { ... });        // ✓ snake_case name
// WRONG: table({ name: 'orderDetail' }, { ... })              // ✗ never camelCase
```

**`ctx.db` accessor uses the JS variable name (camelCase), NOT the SQL name:**

```typescript
// schema({ orderDetail, userStats, eventLog }) → accessors are:
ctx.db.orderDetail.insert({ ... });
ctx.db.userStats.iter();
ctx.db.eventLog.id.find(logId);
```

Options:
- `name` — required, snake_case SQL name
- `public: true` — visible to clients (default: private)
- `event: true` — event table
- `scheduled: (): any => reducerRef` — scheduled table
- `indexes: [{ name, algorithm: 'btree', columns: [...] }]`

## Column Types

| Builder | JS type | Notes |
|---------|---------|-------|
| `t.i32()` | number | |
| `t.i64()` | bigint | Use `0n` literals |
| `t.u32()` | number | |
| `t.u64()` | bigint | Use `0n` literals |
| `t.f32()` | number | |
| `t.f64()` | number | |
| `t.bool()` | boolean | |
| `t.string()` | string | |
| `t.identity()` | Identity | |
| `t.timestamp()` | Timestamp | |
| `t.scheduleAt()` | ScheduleAt | |

Modifiers: `.primaryKey()`, `.autoInc()`, `.unique()`, `.optional()`, `.index('btree')`

Optional columns: `nickname: t.option(t.string())`

## Schema Export

Every module must have exactly this pattern:

```typescript
const spacetimedb = schema({ user, message });
export default spacetimedb;
```

`schema()` takes one object containing all table references. `export default` is mandatory.

## Reducers

Named exports on the schema object. The export name becomes the reducer name:

```typescript
// No arguments — pass just the callback
export const doReset = spacetimedb.reducer((ctx) => { ... });

// With arguments — pass args object, then callback
export const createUser = spacetimedb.reducer(
  { name: t.string(), age: t.i32() },
  (ctx, { name, age }) => {
    ctx.db.user.insert({ id: 0n, name, age, active: true });
  }
);
```

For no-arg reducers, omit the args object entirely — just pass the callback directly.

## DB Operations

```typescript
// Insert (pass 0n for autoInc fields)
ctx.db.user.insert({ id: 0n, name: 'Alice', age: 30 });

// Find by primary key or unique index → row | undefined
ctx.db.user.id.find(userId);
ctx.db.player.identity.find(ctx.sender);

// Filter by btree index → iterator (accessor = column name for inline indexes)
for (const post of ctx.db.post.authorId.filter(authorId)) { }
const posts = [...ctx.db.post.authorId.filter(authorId)];

// Iterate all rows
for (const row of ctx.db.user.iter()) { }
const allUsers = [...ctx.db.user.iter()]; // spread to Array for .sort(), .filter(), .forEach()
// Note: iter() and filter() return IteratorObject, NOT Array. Use [...spread] first.

// Update (spread + override)
const existing = ctx.db.user.id.find(userId);
if (existing) ctx.db.user.id.update({ ...existing, name: newName });

// Delete by primary key value
ctx.db.user.id.delete(userId);
```

## Index Access

**Prefer inline `.index('btree')` on the column** — it's simpler and the accessor
matches the column name. Only use named indexes in `indexes: [...]` for multi-column indexes.
Do NOT use both inline `.index('btree')` AND a named index on the same column — this causes a duplicate name error.

```typescript
// Inline btree index (preferred for single-column):
const post = table({ name: 'post' }, {
  id: t.u64().primaryKey().autoInc(),
  authorId: t.u64().index('btree'),       // inline index
  title: t.string(),
});
// Access by column name:
ctx.db.post.authorId.filter(authorId);

// Multi-column index (must use named index):
const log = table({
  name: 'event_log',
  indexes: [{ name: 'by_category_severity', algorithm: 'btree', columns: ['category', 'severity'] }],
}, { ... });
// Access by index name:
ctx.db.eventLog.by_category_severity.filter(...);

// Primary key — always accessible by column name
ctx.db.user.id.find(1n);

// Unique column
ctx.db.player.identity.find(ctx.sender);
```

## Lifecycle Hooks

```typescript
// Init — runs once on first publish
export const init = spacetimedb.init((ctx) => {
  ctx.db.config.insert({ id: 0, value: 'default' });
});

// Client connected — must be exported
export const onConnect = spacetimedb.clientConnected((ctx) => {
  ctx.db.online.insert({ identity: ctx.sender, connectedAt: ctx.timestamp });
});

// Client disconnected — must be exported
export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  ctx.db.online.identity.delete(ctx.sender);
});
```

`init` uses `spacetimedb.init()`, NOT `spacetimedb.reducer()`.
`clientConnected`/`clientDisconnected` must be `export const`.

The EXPORT NAME determines the reducer name visible in the schema:
✓ `export const onConnect = spacetimedb.clientConnected(...)` → reducer "on_connect"
✗ `export const clientConnected = spacetimedb.clientConnected(...)` → WRONG reducer name

## Authentication

```typescript
// ctx.sender is the caller's Identity
// Compare identities with .equals(), never ===
if (!post.owner.equals(ctx.sender)) throw new Error('unauthorized');
```

## Scheduled Tables

The scheduled table references a reducer, creating a circular dependency.
Use `(): any =>` return type annotation to break the cycle:

```typescript
const tickTimer = table({
  name: 'tick_timer',
  scheduled: (): any => tick,   // (): any => is required
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

const spacetimedb = schema({ tickTimer });
export default spacetimedb;

export const tick = spacetimedb.reducer(
  { timer: tickTimer.rowType },
  (ctx, { timer }) => {
    // timer row is auto-deleted after this reducer runs
  }
);

// Schedule a one-time job (accessor uses JS variable name, not SQL name)
ctx.db.tickTimer.insert({
  scheduledId: 0n,
  scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + delayMicros),
});

// Schedule a repeating job
ctx.db.tickTimer.insert({
  scheduledId: 0n,
  scheduledAt: ScheduleAt.interval(60_000_000n),
});

// Cancel a scheduled job
ctx.db.tickTimer.scheduledId.delete(jobId);
```

## Product Types (Structs)

```typescript
const Position = t.object('Position', { x: t.i32(), y: t.i32() });
const entity = table({ name: 'entity' }, {
  id: t.u64().primaryKey().autoInc(),
  pos: Position,
});
```

## Sum Types (Tagged Unions)

```typescript
const Shape = t.enum('Shape', {
  circle: t.i32(),
  rectangle: t.object('Rect', { w: t.i32(), h: t.i32() }),
});
// Values: { tag: 'circle', value: 10 }
```

## Views

```typescript
// Anonymous view (same for all clients)
export const activeAnnouncements = spacetimedb.anonymousView(
  { name: 'active_announcements', public: true },
  t.array(announcement.rowType),
  (ctx) => Array.from(ctx.db.announcement.active.filter(true))
);

// Per-user view (varies by ctx.sender)
export const my_profile = spacetimedb.view(
  { name: 'my_profile', public: true },
  t.option(profile.rowType),
  (ctx) => ctx.db.profile.identity.find(ctx.sender) ?? undefined
);
```

## Complete Example

```typescript
import { schema, table, t } from 'spacetimedb/server';

const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
  }
);

const message = table(
  {
    name: 'message',
    public: true,
    indexes: [{ name: 'message_sender', algorithm: 'btree', columns: ['sender'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    sender: t.identity(),
    text: t.string(),
  }
);

const spacetimedb = schema({ user, message });
export default spacetimedb;

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) ctx.db.user.identity.update({ ...existing, online: true });
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) ctx.db.user.identity.update({ ...existing, online: false });
});

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (ctx.db.user.identity.find(ctx.sender)) throw new Error('already registered');
    ctx.db.user.insert({ identity: ctx.sender, name, online: true });
  }
);

export const sendMessage = spacetimedb.reducer(
  { text: t.string() },
  (ctx, { text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new Error('not registered');
    ctx.db.message.insert({ id: 0n, sender: ctx.sender, text });
  }
);
```
