# SpacetimeDB TypeScript SDK Reference

## Imports

```typescript
import { schema, table, t } from 'spacetimedb/server';
import { SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';        // for scheduled tables only
```

## Tables

`table(OPTIONS, COLUMNS)` — two arguments. The `name` field MUST be snake_case:

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

Options: `name` (snake_case, required), `public: true`, `event: true`, `scheduled: (): any => reducerRef`, `indexes: [...]`

`ctx.db` accessors use the JS variable name (camelCase), not the SQL name.

## Column Types

| Builder | JS type | Notes |
|---------|---------|-------|
| `t.u64()` | bigint | Use `0n` literals |
| `t.i64()` | bigint | Use `0n` literals |
| `t.u32()` / `t.i32()` | number | |
| `t.f64()` / `t.f32()` | number | |
| `t.bool()` | boolean | |
| `t.string()` | string | |
| `t.identity()` | Identity | |
| `t.timestamp()` | Timestamp | |
| `t.scheduleAt()` | ScheduleAt | |

Modifiers: `.primaryKey()`, `.autoInc()`, `.unique()`, `.index('btree')`

Optional columns: `nickname: t.option(t.string())`

## Indexes

Prefer inline `.index('btree')` for single-column. Use named indexes only for multi-column:

```typescript
// Inline (preferred):
authorId: t.u64().index('btree'),
// Access: ctx.db.post.authorId.filter(authorId);

// Multi-column (named):
indexes: [{ accessor: 'by_cat_sev', algorithm: 'btree', columns: ['category', 'severity'] }]
```

## Schema Export

```typescript
const spacetimedb = schema({ entity, record });  // ONE object, not spread args
export default spacetimedb;
```

## Reducers

Export name becomes the reducer name:

```typescript
export const createEntity = spacetimedb.reducer(
  { name: t.string(), age: t.i32() },
  (ctx, { name, age }) => {
    ctx.db.entity.insert({ identity: ctx.sender, name, age, active: true });
  }
);

// No arguments — just the callback:
export const doReset = spacetimedb.reducer((ctx) => { ... });
```

## DB Operations

```typescript
ctx.db.entity.insert({ id: 0n, name: 'Sample' });          // Insert (0n for autoInc)
ctx.db.entity.id.find(entityId);                           // Find by PK → row | null
ctx.db.entity.identity.find(ctx.sender);                   // Find by unique column
[...ctx.db.item.authorId.filter(authorId)];                // Filter → spread to Array
[...ctx.db.entity.iter()];                                 // All rows → Array
ctx.db.entity.id.update({ ...existing, name: newName });   // Update (spread + override)
ctx.db.entity.id.delete(entityId);                         // Delete by PK
```

Note: `iter()` and `filter()` return iterators. Spread to Array for `.sort()`, `.filter()`, `.map()`.

## Lifecycle Hooks

MUST be `export const` — bare calls are silently ignored:

```typescript
export const init = spacetimedb.init((ctx) => { ... });
export const onConnect = spacetimedb.clientConnected((ctx) => { ... });
export const onDisconnect = spacetimedb.clientDisconnected((ctx) => { ... });
```

## Authentication & Timestamps

```typescript
// Auth: ctx.sender is the caller's Identity
if (!row.owner.equals(ctx.sender)) throw new SenderError('unauthorized');

// Server timestamps
ctx.db.item.insert({ id: 0n, createdAt: ctx.timestamp });

// Client: Timestamp → Date
new Date(Number(row.createdAt.microsSinceUnixEpoch / 1000n));
```

## Scheduled Tables

```typescript
const tickTimer = table({
  name: 'tick_timer',
  scheduled: (): any => tick,   // (): any => breaks circular dep
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

export const tick = spacetimedb.reducer(
  { timer: tickTimer.rowType },
  (ctx, { timer }) => { /* timer row auto-deleted after this runs */ }
);

// One-time: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + delayMicros)
// Repeating: ScheduleAt.interval(60_000_000n)
```

## React Client

### main.tsx — SpacetimeDBProvider is required

```typescript
import React, { useMemo } from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';
import App from './App';

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

ReactDOM.createRoot(document.getElementById('root')!).render(<Root />);
```

### App.tsx patterns

```typescript
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  // Save auth token
  useEffect(() => { if (token) localStorage.setItem('auth_token', token); }, [token]);

  // Subscribe when connected
  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe(['SELECT * FROM entity', 'SELECT * FROM record']);
  }, [conn, isActive]);

  // Reactive data
  const [entities] = useTable(tables.entity);
  const [records] = useTable(tables.record);

  // Call reducers with object syntax
  conn?.reducers.addRecord({ data });

  // Compare identities
  const isMe = row.owner.toHexString() === myIdentity?.toHexString();
}
```

## Complete Example

```typescript
// schema.ts
import { schema, table, t } from 'spacetimedb/server';

const entity = table({ name: 'entity', public: true }, {
  identity: t.identity().primaryKey(),
  name: t.string(),
  active: t.bool(),
});

const record = table({ name: 'record', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  owner: t.identity(),
  value: t.u32(),
  createdAt: t.timestamp(),
});

const spacetimedb = schema({ entity, record });
export default spacetimedb;
```

```typescript
// index.ts
import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
export { default } from './schema';

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.entity.identity.find(ctx.sender);
  if (existing) ctx.db.entity.identity.update({ ...existing, active: true });
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.entity.identity.find(ctx.sender);
  if (existing) ctx.db.entity.identity.update({ ...existing, active: false });
});

export const createEntity = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (ctx.db.entity.identity.find(ctx.sender)) throw new SenderError('already exists');
    ctx.db.entity.insert({ identity: ctx.sender, name, active: true });
  }
);

export const addRecord = spacetimedb.reducer(
  { value: t.u32() },
  (ctx, { value }) => {
    if (!ctx.db.entity.identity.find(ctx.sender)) throw new SenderError('not found');
    ctx.db.record.insert({ id: 0n, owner: ctx.sender, value, createdAt: ctx.timestamp });
  }
);
```
