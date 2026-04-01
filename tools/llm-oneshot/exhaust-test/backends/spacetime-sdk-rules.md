# SpacetimeDB TypeScript SDK Reference

**USE THIS REFERENCE for all SpacetimeDB code. Do NOT guess SDK syntax from memory — the API has unique patterns that differ from what you may expect.**

## Imports

```typescript
import { schema, table, t } from 'spacetimedb/server';
import { SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';        // for scheduled tables only
```

CRITICAL: The `name` field in table() MUST be snake_case (e.g. 'order_detail', NOT 'orderDetail').
The JS variable can be camelCase, the `name` string cannot.

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

**`ctx.db` accessor uses the JS variable name (camelCase), NOT the SQL name:**

```typescript
// schema({ orderDetail, userStats, eventLog }) -> accessors are:
ctx.db.orderDetail.insert({ ... });
ctx.db.userStats.iter();
ctx.db.eventLog.id.find(logId);
```

Options:
- `name` — required, snake_case SQL name
- `public: true` — visible to clients (default: private)
- `event: true` — event table
- `scheduled: (): any => reducerRef` — scheduled table
- `indexes: [{ accessor, algorithm: 'btree', columns: [...] }]`

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

Modifiers: `.primaryKey()`, `.autoInc()`, `.unique()`, `.index('btree')`

Optional columns: `nickname: t.option(t.string())` — wrap with `t.option()`, NOT `.optional()` (does not exist).

## Index Definitions

**Use `accessor:` NOT `name:` for the index property name.**

```typescript
// Inline btree index (preferred for single-column):
const post = table({ name: 'post', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  authorId: t.u64().index('btree'),       // inline index
  title: t.string(),
});
// Access by column name:
ctx.db.post.authorId.filter(authorId);

// Multi-column index (must use named index with accessor):
const log = table({
  name: 'event_log', public: true,
  indexes: [{ accessor: 'by_category_severity', algorithm: 'btree', columns: ['category', 'severity'] }],
}, { ... });
// Access by accessor name:
ctx.db.eventLog.by_category_severity.filter(...);

// Primary key — always accessible by column name
ctx.db.user.id.find(1n);
ctx.db.player.identity.find(ctx.sender);
```

Prefer inline `.index('btree')` on the column for single-column indexes. Only use named indexes for multi-column.
Do NOT use both inline `.index('btree')` AND a named index on the same column — causes duplicate name error.

## Schema Export

Every module must have exactly this pattern:

```typescript
// schema() takes ONE OBJECT — NEVER spread args
const spacetimedb = schema({ user, message });
export default spacetimedb;
```

**WRONG:** `schema(user, message)` — spread args do NOT work. Always use an object.

## Reducers

Named exports on the schema object. The **export name** becomes the reducer name:

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

**WRONG:** `spacetimedb.reducer('createUser', { ... }, fn)` — do NOT pass a string name as first arg.

For no-arg reducers, omit the args object entirely — just pass the callback directly.

## DB Operations

```typescript
// Insert (pass 0n for autoInc fields)
ctx.db.user.insert({ id: 0n, name: 'Alice', age: 30 });

// Find by primary key or unique index -> row | null (NOT undefined)
ctx.db.user.id.find(userId);
ctx.db.player.identity.find(ctx.sender);

// Filter by btree index -> iterator (accessor = column name for inline indexes)
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

## Lifecycle Hooks

**MUST be `export const`. Bare calls without export are SILENTLY IGNORED.**

```typescript
// Init — runs once on first publish
export const init = spacetimedb.init((ctx) => {
  ctx.db.config.insert({ id: 0, value: 'default' });
});

// Client connected — MUST be exported
export const onConnect = spacetimedb.clientConnected((ctx) => {
  ctx.db.online.insert({ identity: ctx.sender, connectedAt: ctx.timestamp });
});

// Client disconnected — MUST be exported
export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  ctx.db.online.identity.delete(ctx.sender);
});
```

`init` uses `spacetimedb.init()`, NOT `spacetimedb.reducer()`.
`clientConnected`/`clientDisconnected` must be `export const`.

The EXPORT NAME determines the reducer name visible in the schema:
- CORRECT: `export const onConnect = spacetimedb.clientConnected(...)` -> reducer "on_connect"
- WRONG: `export const clientConnected = spacetimedb.clientConnected(...)` -> wrong reducer name

## Authentication

```typescript
// ctx.sender is the caller's Identity
// Compare identities with .equals(), never ===
if (!post.owner.equals(ctx.sender)) throw new SenderError('unauthorized');
```

## Timestamps

```typescript
// Server-side: use ctx.timestamp for current time
ctx.db.item.insert({ id: 0n, createdAt: ctx.timestamp });

// Client-side: Timestamp is an object, NOT a number
const date = new Date(Number(row.createdAt.microsSinceUnixEpoch / 1000n));
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

// Schedule a one-time job
ctx.db.tickTimer.insert({
  scheduledId: 0n,
  scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + delayMicros),
});

// Schedule a repeating job
ctx.db.tickTimer.insert({
  scheduledId: 0n,
  scheduledAt: ScheduleAt.interval(60_000_000n),
});
```

## React Client (CRITICAL — follow this exactly)

### main.tsx — SpacetimeDBProvider is REQUIRED

**Every app MUST wrap the root component with `SpacetimeDBProvider`.** Without it, `useTable` crashes.

```typescript
// main.tsx
import React, { useMemo } from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';
import App from './App';
import './styles.css';

function Root() {
  const connectionBuilder = useMemo(() =>
    DbConnection.builder()
      .withUri(SPACETIMEDB_URI)
      .withDatabaseName(MODULE_NAME)
      .withToken(localStorage.getItem('auth_token') || undefined),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  );
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>
);
```

**WRONG:** Rendering `<App />` without `SpacetimeDBProvider` — useTable will throw.
**WRONG:** Calling `builder.build()` manually — the provider calls it internally.

### App.tsx — useSpacetimeDB + useTable + subscriptions

```typescript
import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';

function App() {
  // Get connection state from provider
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;
  const [subscribed, setSubscribed] = useState(false);

  // Save auth token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe to tables when connected
  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM message',
      ]);
  }, [conn, isActive]);

  // useTable returns [rows, isLoading] — works because SpacetimeDBProvider is above
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [messages] = useTable(tables.message);

  // Call reducers via conn.reducers with OBJECT syntax
  const handleSend = () => {
    conn?.reducers.sendMessage({ roomId: selectedRoomId, text: messageText });
  };

  // Compare identities using toHexString()
  const isMe = msg.sender.toHexString() === myIdentity?.toHexString();
  // ...
}
```

### Key client patterns

- `useSpacetimeDB()` returns `{ isActive, identity, token, getConnection }` — always use this, NEVER build connection manually
- `getConnection()` returns the `DbConnection` instance for calling reducers
- `useTable(tables.user)` returns `[rows, isLoading]` — must be inside `SpacetimeDBProvider`
- Reducers use **object syntax**: `conn.reducers.foo({ param: 'value' })` — NEVER positional args
- Identity comparison: `a.toHexString() === b.toHexString()` — NEVER use `===` directly
- Timestamp to Date: `new Date(Number(row.createdAt.microsSinceUnixEpoch / 1000n))`
- Subscribe in `useEffect` when `conn && isActive`, call `conn.subscriptionBuilder().subscribe([...])`

## Hallucinated APIs — DO NOT USE

These do NOT exist in SpacetimeDB:
- `@clockworklabs/spacetimedb-sdk` -> use `spacetimedb`
- `SpacetimeDBClient.connect()` -> use `DbConnection.builder()` inside SpacetimeDBProvider
- `conn.reducers.foo("val")` -> use `conn.reducers.foo({ param: "val" })`
- `User.filterByName()` -> use `ctx.db.user.iter()` + manual filter
- `.on('initialStateSync')` -> use `.onApplied()`
- `import { SpacetimeDBClient } from '...'` -> does not exist
- `builder.build()` in React -> use `SpacetimeDBProvider` instead (it calls build internally)

## Complete Example

```typescript
// schema.ts
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
    indexes: [{ accessor: 'message_sender', algorithm: 'btree', columns: ['sender'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    sender: t.identity(),
    text: t.string(),
  }
);

const spacetimedb = schema({ user, message });
export default spacetimedb;
```

```typescript
// index.ts
import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
export { default } from './schema';

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
    if (ctx.db.user.identity.find(ctx.sender)) throw new SenderError('already registered');
    ctx.db.user.insert({ identity: ctx.sender, name, online: true });
  }
);

export const sendMessage = spacetimedb.reducer(
  { text: t.string() },
  (ctx, { text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('not registered');
    ctx.db.message.insert({ id: 0n, sender: ctx.sender, text });
  }
);
```
