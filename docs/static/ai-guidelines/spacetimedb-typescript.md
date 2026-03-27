# SpacetimeDB TypeScript Server Module Guidelines

## Imports

```typescript
import { schema, table, t } from 'spacetimedb/server';
```

Additional imports when needed:
```typescript
import { ScheduleAt } from 'spacetimedb';        // For scheduled tables
import { Timestamp } from 'spacetimedb';          // For timestamp arithmetic
```

## Table Definitions

Tables are defined with `table(OPTIONS, COLUMNS)` — two arguments:

```typescript
const user = table(
  {
    name: 'user',
    public: true,
    indexes: [{ name: 'user_email', algorithm: 'btree', columns: ['email'] }]
  },
  {
    id: t.u64().primaryKey().autoInc(),
    email: t.string(),
    name: t.string(),
    active: t.bool(),
    createdAt: t.timestamp(),
  }
);
```

Options (first argument):
- `name` — required, the table's SQL name
- `public` — optional, makes table visible to clients (default: private)
- `event` — optional, marks as event table
- `scheduled` — optional, `() => reducerRef` for scheduled tables
- `indexes` — optional, array of `{ name, algorithm: 'btree', columns: [...] }`

## Column Types

| Type | Usage | Notes |
|------|-------|-------|
| `t.i32()` | `t.i32()` | Signed 32-bit integer |
| `t.i64()` | `t.i64()` | Signed 64-bit (BigInt in JS) |
| `t.u32()` | `t.u32()` | Unsigned 32-bit integer |
| `t.u64()` | `t.u64()` | Unsigned 64-bit (BigInt in JS) |
| `t.f32()` | `t.f32()` | 32-bit float |
| `t.f64()` | `t.f64()` | 64-bit float |
| `t.bool()` | `t.bool()` | Boolean |
| `t.string()` | `t.string()` | Text |
| `t.identity()` | `t.identity()` | User identity |
| `t.timestamp()` | `t.timestamp()` | Timestamp |
| `t.scheduleAt()` | `t.scheduleAt()` | Schedule metadata |

Column modifiers:
```typescript
t.u64().primaryKey()            // Primary key
t.u64().primaryKey().autoInc()  // Auto-incrementing primary key
t.string().unique()             // Unique constraint
t.string().optional()           // Nullable (Option type)
t.string().index('btree')       // Inline btree index
```

Optional fields use `t.option()`:
```typescript
nickname: t.option(t.string()),   // nullable string
score: t.option(t.i32()),         // nullable i32
```

All `u64` and `i64` fields use JavaScript BigInt literals: `0n`, `1n`, `100n`.

## Product Types (Structs)

```typescript
const Position = t.object('Position', { x: t.i32(), y: t.i32() });
const Velocity = t.object('Velocity', { dx: t.f32(), dy: t.f32() });

const entity = table({ name: 'entity' }, {
  id: t.u64().primaryKey().autoInc(),
  pos: Position,
  vel: Velocity,
});
```

## Sum Types (Tagged Unions)

```typescript
const Shape = t.enum('Shape', {
  circle: t.i32(),
  rectangle: t.object('Rect', { w: t.i32(), h: t.i32() }),
});

// Values use { tag: 'variant', value: payload }
const circle = { tag: 'circle', value: 10 };
const rect = { tag: 'rectangle', value: { w: 5, h: 3 } };
```

## Schema Export

Every module must export a schema containing all tables:

```typescript
const spacetimedb = schema({ user, product, note });
export default spacetimedb;
```

The `schema()` function takes exactly one argument: an object with all table references.

## Reducers

Reducers are exported named constants defined on the schema object:

```typescript
// With arguments
export const createUser = spacetimedb.reducer(
  { name: t.string(), age: t.i32() },
  (ctx, { name, age }) => {
    ctx.db.user.insert({ id: 0n, name, age, active: true, createdAt: ctx.timestamp });
  }
);

// Without arguments (empty object)
export const resetAll = spacetimedb.reducer({}, (ctx) => {
  for (const row of ctx.db.user.iter()) {
    ctx.db.user.id.delete(row.id);
  }
});
```

Reducer names come from the export name, not from a string argument.

## Database Operations

### Insert
```typescript
ctx.db.user.insert({ id: 0n, name: 'Alice', age: 30, active: true });
// insert() returns the inserted row
const row = ctx.db.user.insert({ id: 0n, name: 'Bob', age: 25, active: true });
const newId = row.id;
```

For auto-increment fields, pass `0n` as a placeholder.

### Find (by primary key or unique index)
```typescript
const user = ctx.db.user.id.find(userId);       // returns row or undefined
const player = ctx.db.player.identity.find(ctx.sender);
```

### Filter (by btree index — returns iterator)
```typescript
const msgs = [...ctx.db.message.message_room_id.filter(roomId)];
// or
for (const msg of ctx.db.message.message_room_id.filter(roomId)) {
  // process each message
}
// or
const msgs = Array.from(ctx.db.message.message_room_id.filter(roomId));
```

### Iterate all rows
```typescript
for (const row of ctx.db.user.iter()) {
  // process each row
}
```

### Update (by primary key — spread existing row, override fields)
```typescript
const existing = ctx.db.user.id.find(userId);
if (!existing) throw new Error('not found');
ctx.db.user.id.update({ ...existing, name: newName, active: false });
```

### Delete (by primary key value)
```typescript
ctx.db.user.id.delete(userId);
ctx.db.player.identity.delete(ctx.sender);
```

## Index Access

Index names are used verbatim as property accessors on `ctx.db.tableName`:

```typescript
// Primary key lookup
ctx.db.user.id.find(userId);

// Named index filter
ctx.db.message.message_room_id.filter(roomId);

// Inline index (column-level)
// If defined as: owner: t.identity().index('btree')
// Access as: ctx.db.task.owner.filter(ctx.sender)
```

For indexes defined in the `indexes` option, access by exact index name:
```typescript
// Definition: indexes: [{ name: 'user_email', algorithm: 'btree', columns: ['email'] }]
// Access:
ctx.db.user.user_email.find(emailValue);
```

## Authentication

`ctx.sender` is the authenticated identity of the caller:

```typescript
export const createPost = spacetimedb.reducer(
  { text: t.string() },
  (ctx, { text }) => {
    ctx.db.post.insert({ id: 0n, owner: ctx.sender, text, createdAt: ctx.timestamp });
  }
);

export const deletePost = spacetimedb.reducer(
  { postId: t.u64() },
  (ctx, { postId }) => {
    const post = ctx.db.post.id.find(postId);
    if (!post) throw new Error('not found');
    if (!post.owner.equals(ctx.sender)) throw new Error('unauthorized');
    ctx.db.post.id.delete(postId);
  }
);
```

Always compare identities with `.equals()`, not `===`.

## Lifecycle Hooks

```typescript
export const init = spacetimedb.init((ctx) => {
  // Runs once when the module is first published
  ctx.db.config.insert({ id: 0n, setting: 'default' });
});

export const onConnect = spacetimedb.clientConnected((ctx) => {
  // ctx.sender — the connecting client's identity
  ctx.db.onlinePlayer.insert({ identity: ctx.sender, connectedAt: ctx.timestamp });
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  ctx.db.onlinePlayer.identity.delete(ctx.sender);
});
```

## Views

```typescript
// Anonymous view (same result for all clients)
export const activeAnnouncements = spacetimedb.anonymousView(
  { name: 'active_announcements', public: true },
  t.array(announcement.rowType),
  (ctx) => {
    return Array.from(ctx.db.announcement.active.filter(true));
  }
);

// Per-user view (result varies by ctx.sender)
spacetimedb.view(
  { name: 'my_items', public: true },
  t.array(item.rowType),
  (ctx) => {
    return [...ctx.db.item.owner.filter(ctx.sender)];
  }
);
```

Views should use index lookups for efficiency.

## Scheduled Tables

```typescript
const reminderTable = table(
  {
    name: 'reminder',
    scheduled: (): any => fireReminder,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    message: t.string(),
  }
);

export const fireReminder = spacetimedb.reducer(
  { timer: reminderTable.rowType },
  (ctx, { timer }) => {
    // timer.message, timer.scheduledId available
    // Row is auto-deleted after reducer completes
  }
);

// Schedule a one-time job
export const scheduleReminder = spacetimedb.reducer(
  { message: t.string(), delayMicros: t.u64() },
  (ctx, { message, delayMicros }) => {
    const fireAt = ctx.timestamp.microsSinceUnixEpoch + delayMicros;
    ctx.db.reminder.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(fireAt),
      message,
    });
  }
);

// Schedule a repeating job (interval in microseconds)
ctx.db.reminder.insert({
  scheduledId: 0n,
  scheduledAt: ScheduleAt.interval(60_000_000n), // every 60 seconds
  message: 'periodic',
});

// Cancel a scheduled job
ctx.db.reminder.scheduledId.delete(jobId);
```

## Complete Module Example

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
    indexes: [{ name: 'message_sender', algorithm: 'btree', columns: ['sender'] }]
  },
  {
    id: t.u64().primaryKey().autoInc(),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
  }
);

const spacetimedb = schema({ user, message });
export default spacetimedb;

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: true });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false });
  }
});

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (ctx.db.user.identity.find(ctx.sender)) {
      throw new Error('already registered');
    }
    ctx.db.user.insert({ identity: ctx.sender, name, online: true });
  }
);

export const sendMessage = spacetimedb.reducer(
  { text: t.string() },
  (ctx, { text }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new Error('not registered');
    ctx.db.message.insert({ id: 0n, sender: ctx.sender, text, sentAt: ctx.timestamp });
  }
);
```
