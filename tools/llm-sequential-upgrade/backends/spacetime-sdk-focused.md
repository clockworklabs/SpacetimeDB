# SpacetimeDB TypeScript SDK Reference (focused)

Lean, chat-app-focused SDK reference for the SpacetimeDB **2.x** TypeScript SDK —
parity in scope/prescriptiveness with the PostgreSQL/MongoDB backend files. Covers only
what this app needs; omits SDK features the app doesn't use.

## Imports

```typescript
import { schema, table, t } from 'spacetimedb/server';
import { SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';        // scheduled tables only
```

## Tables

`table(OPTIONS, COLUMNS)` — two arguments. `name` is snake_case.

```typescript
const user = table(
  { name: 'user', public: true },
  { identity: t.identity().primaryKey(), name: t.string(), online: t.bool() }
);
```

Options: `name` (snake_case), `public: true`, `scheduled: (): any => reducerRef`, `indexes: [...]`.
`ctx.db` accessors use the **camelCase** form of the table's `name`.

## Column Types

`t.u64()`/`t.i64()` → bigint (use `0n` literals) · `t.u32()`/`t.i32()`/`t.f64()` → number ·
`t.bool()` · `t.string()` · `t.identity()` → Identity · `t.timestamp()` → Timestamp ·
`t.scheduleAt()` → ScheduleAt · optional: `t.option(t.string())`

Modifiers: `.primaryKey()` `.autoInc()` `.unique()` `.index('btree')`

## Indexes

```typescript
// single-column inline (preferred):
authorId: t.u64().index('btree'),               // → ctx.db.post.authorId.filter(authorId)
// multi-column (named):
indexes: [{ accessor: 'by_room_user', algorithm: 'btree', columns: ['roomId', 'userIdentity'] }]
// → ctx.db.draft.by_room_user.filter([roomId, identity])
```

## Schema Export

```typescript
const spacetimedb = schema({ user, room, message });   // ONE object, not spread args
export default spacetimedb;
```

## Reducers

Export name becomes the reducer name.

```typescript
export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    ctx.db.message.insert({ id: 0n, roomId, sender: ctx.sender, text, sentAt: ctx.timestamp });
  }
);
// no arguments — just the callback:
export const reset = spacetimedb.reducer((ctx) => { ... });
```

## DB Operations

```typescript
ctx.db.message.insert({ id: 0n, ... });        // insert (0n for autoInc PK)
ctx.db.message.id.find(msgId);                  // by PK → row | null
ctx.db.user.identity.find(ctx.sender);          // by unique column
[...ctx.db.message.roomId.filter(roomId)];      // filter → spread to Array
[...ctx.db.message.iter()];                      // all rows → Array
ctx.db.message.id.update({ ...existing, text }); // update (spread + override)
ctx.db.message.id.delete(msgId);
```

`iter()`/`filter()` return iterators — spread to Array for `.sort()`/`.map()`/`.filter()`.

## Lifecycle Hooks

MUST be `export const` (bare calls are silently ignored).

```typescript
export const init = spacetimedb.init((ctx) => { ... });
export const onConnect = spacetimedb.clientConnected((ctx) => { ... });
export const onDisconnect = spacetimedb.clientDisconnected((ctx) => { ... });
```

## Reducer Context — identity, time, randomness

Inside a reducer, get sender / time / randomness **only** from `ctx`.
**Standard-library clocks and random sources (`Date.now()`, `Math.random()`) are NOT available
in modules** — use `ctx` instead.

```typescript
ctx.sender                                    // caller Identity
if (!row.owner.equals(ctx.sender)) throw new SenderError('unauthorized');
ctx.timestamp                                 // deterministic server time
ctx.db.message.insert({ ..., createdAt: ctx.timestamp });
ctx.random();                                 // [0.0, 1.0)
ctx.random.integerInRange(1, 6);              // inclusive
// Client: Timestamp → Date
new Date(Number(row.createdAt.microsSinceUnixEpoch / 1000n));
```

## Scheduled Tables (timers)

```typescript
const tickTimer = table(
  { name: 'tick_timer', scheduled: (): any => tick },   // (): any => breaks circular dep
  { scheduledId: t.u64().primaryKey().autoInc(), scheduledAt: t.scheduleAt() }
);
export const tick = spacetimedb.reducer(
  { timer: tickTimer.rowType },
  (ctx, { timer }) => { /* timer row auto-deleted after this runs */ }
);
// one-shot:   ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + delayMicros)
// repeating:  ScheduleAt.interval(60_000_000n)
```

## React Client — main.tsx

```typescript
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';

const connectionBuilder = useMemo(() =>
  DbConnection.builder()
    .withUri(SPACETIMEDB_URI)
    .withDatabaseName(MODULE_NAME)
    .withToken(localStorage.getItem('auth_token') || undefined),
  []);
// <SpacetimeDBProvider connectionBuilder={connectionBuilder}><App /></SpacetimeDBProvider>
```

## React Client — App.tsx

```typescript
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
const conn = getConnection() as DbConnection | null;

useEffect(() => { if (token) localStorage.setItem('auth_token', token); }, [token]);

useEffect(() => {                              // subscribe once connected
  if (!conn || !isActive) return;
  conn.subscriptionBuilder()
    .onApplied(() => setReady(true))
    .subscribe([tables.user, tables.message]); // typed tables (raw SQL strings also accepted)
}, [conn, isActive]);

const [users] = useTable(tables.user);         // reactive rows; returns [rows, isReady]
const [messages] = useTable(tables.message);

conn?.reducers.sendMessage({ roomId, text });  // call reducers with object args
const isMe = row.owner.toHexString() === myIdentity?.toHexString();
```
