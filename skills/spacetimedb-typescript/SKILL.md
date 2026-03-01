---
name: spacetimedb-typescript
description: Build TypeScript clients for SpacetimeDB. Use when connecting to SpacetimeDB from web apps, Node.js, Deno, Bun, or other JavaScript runtimes.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "1.0"
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
| `ctx.myTable` in procedure tx | `tx.db.myTable` | Wrong context variable |

### Client-side errors

| Wrong | Right | Error |
|-------|-------|-------|
| `@spacetimedb/sdk` | `spacetimedb` | 404 / missing subpath |
| `conn.reducers.foo("val")` | `conn.reducers.foo({ param: "val" })` | Wrong reducer syntax |
| Inline `connectionBuilder` | `useMemo(() => ..., [])` | Reconnects every render |
| `const rows = useTable(table)` | `const [rows, isLoading] = useTable(table)` | Tuple destructuring |
| Optimistic UI updates | Let subscriptions drive state | Desync issues |
| `<SpacetimeDBProvider builder={...}>` | `connectionBuilder={...}` | Wrong prop name |

---

## Hard Requirements

1. **DO NOT edit generated bindings** — regenerate with `spacetime generate`
2. **Reducers are transactional** — they do not return data
3. **Reducers must be deterministic** — no filesystem, network, timers, random
4. **Reducer calls use object syntax** — `{ param: 'value' }` not positional args
5. **Import `DbConnection` from `./module_bindings`** — not from `spacetimedb`
6. **useTable returns a tuple** — `const [rows, isLoading] = useTable(tables.myTable)`
7. **Memoize connectionBuilder** — wrap in `useMemo(() => ..., [])` to prevent reconnects
8. **Views can only use index lookups** — `.iter()` is not allowed in views

---

## Installation

```bash
npm install spacetimedb
# or
pnpm add spacetimedb
# or
yarn add spacetimedb
```

For Node.js 18-21, install the `undici` peer dependency:

```bash
npm install spacetimedb undici
```

Node.js 22+ and browser environments work out of the box.

## Generating Type Bindings

Before using the SDK, generate TypeScript bindings from your SpacetimeDB module:

```bash
spacetime generate --lang typescript --out-dir ./src/module_bindings --project-path ./server
```

This creates a `module_bindings` directory with:
- `index.ts` - Main exports including `DbConnection`, `tables`, `reducers`, `query`
- Type definitions for each table (e.g., `player_table.ts`, `user_table.ts`)
- Type definitions for each reducer (e.g., `create_player_reducer.ts`)
- Custom type definitions (e.g., `point_type.ts`)

## Basic Connection Setup

```typescript
import { DbConnection } from './module_bindings';

const connection = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('my_database')
  .onConnect((conn, identity, token) => {
    console.log('Connected with identity:', identity.toHexString());

    // Store token for reconnection
    localStorage.setItem('spacetimedb_token', token);

    // Subscribe to tables after connection
    conn.subscriptionBuilder().subscribe('SELECT * FROM player');
  })
  .onDisconnect((ctx) => {
    console.log('Disconnected');
  })
  .onConnectError((ctx, error) => {
    console.error('Connection error:', error);
  })
  .build();
```

## Connection Builder Options

```typescript
DbConnection.builder()
  // Required: SpacetimeDB server URI
  .withUri('ws://localhost:3000')

  // Required: Database module name or address
  .withModuleName('my_database')

  // Optional: Authentication token for reconnection
  .withToken(localStorage.getItem('spacetimedb_token') ?? undefined)

  // Optional: Enable compression (default: 'gzip')
  .withCompression('gzip')  // or 'none'

  // Optional: Light mode reduces network traffic
  .withLightMode(true)

  // Optional: Wait for durable writes before receiving updates
  .withConfirmedReads(true)

  // Connection lifecycle callbacks
  .onConnect((conn, identity, token) => { /* ... */ })
  .onDisconnect((ctx, error) => { /* ... */ })
  .onConnectError((ctx, error) => { /* ... */ })

  .build();
```

## Subscribing to Tables

Subscriptions sync table data to the client cache. Use SQL queries to filter what data you receive.

### Basic Subscription

```typescript
connection.subscriptionBuilder()
  .onApplied((ctx) => {
    console.log('Subscription applied, cache is ready');
  })
  .onError((ctx, error) => {
    console.error('Subscription error:', error);
  })
  .subscribe('SELECT * FROM player');
```

### Multiple Queries

```typescript
connection.subscriptionBuilder()
  .subscribe([
    'SELECT * FROM player',
    'SELECT * FROM game_state',
    'SELECT * FROM message WHERE room_id = 1'
  ]);
```

### Typed Query Builder

Use the generated `query` object for type-safe queries:

```typescript
import { query } from './module_bindings';

// Simple query - selects all rows
connection.subscriptionBuilder()
  .subscribe(query.player.build());

// Query with WHERE clause
connection.subscriptionBuilder()
  .subscribe(
    query.player
      .where(row => row.name.eq('Alice'))
      .build()
  );

// Complex conditions
connection.subscriptionBuilder()
  .subscribe(
    query.player
      .where(row => row.score.gte(100))
      .where(row => row.isActive.eq(true))
      .build()
  );
```

### Subscribe to All Tables

For development or small datasets:

```typescript
connection.subscriptionBuilder().subscribeToAllTables();
```

### Unsubscribing

```typescript
const handle = connection.subscriptionBuilder()
  .onApplied(() => console.log('Subscribed'))
  .subscribe('SELECT * FROM player');

// Later, unsubscribe
handle.unsubscribe();

// Or with callback when complete
handle.unsubscribeThen((ctx) => {
  console.log('Unsubscribed successfully');
});
```

## Accessing Table Data

After subscription, access cached data through `connection.db`:

```typescript
// Iterate all rows
for (const player of connection.db.player.iter()) {
  console.log(player.name, player.score);
}

// Convert to array
const players = Array.from(connection.db.player.iter());

// Count rows
const count = connection.db.player.count();

// Find by primary key (if table has one)
const player = connection.db.player.id.find(42);

// Find by indexed column
const alice = connection.db.player.name.find('Alice');
```

## Table Event Callbacks

Listen for real-time changes to table data:

```typescript
// Row inserted
connection.db.player.onInsert((ctx, player) => {
  console.log('New player:', player.name);
});

// Row deleted
connection.db.player.onDelete((ctx, player) => {
  console.log('Player left:', player.name);
});

// Row updated (requires primary key on table)
connection.db.player.onUpdate((ctx, oldPlayer, newPlayer) => {
  console.log(`${oldPlayer.name} score: ${oldPlayer.score} -> ${newPlayer.score}`);
});

// Remove callbacks
const onInsertCb = (ctx, player) => console.log(player);
connection.db.player.onInsert(onInsertCb);
connection.db.player.removeOnInsert(onInsertCb);
```

### Event Context

Callbacks receive an `EventContext` with information about the event:

```typescript
connection.db.player.onInsert((ctx, player) => {
  // Access to database
  const allPlayers = Array.from(ctx.db.player.iter());

  // Check event type
  if (ctx.event.tag === 'Reducer') {
    const { callerIdentity, reducer, status } = ctx.event.value;
    console.log(`Triggered by reducer: ${reducer.name}`);
  }

  // Call other reducers
  ctx.reducers.sendMessage({ playerId: player.id, text: 'Welcome!' });
});
```

## Calling Reducers

Reducers are server-side functions that modify the database. **CRITICAL: Use object syntax, not positional arguments.**

```typescript
// CORRECT: Object syntax
connection.reducers.createPlayer({ name: 'Alice', location: { x: 0, y: 0 } });

// WRONG: Positional arguments
// connection.reducers.createPlayer('Alice', { x: 0, y: 0 });  // DO NOT DO THIS

// Listen for reducer results
connection.reducers.onCreatePlayer((ctx, args) => {
  const { callerIdentity, status, timestamp, energyConsumed } = ctx.event;

  if (status.tag === 'Committed') {
    console.log('Player created successfully');
  } else if (status.tag === 'Failed') {
    console.error('Failed:', status.value);
  }
});

// Remove reducer callback
connection.reducers.removeOnCreatePlayer(callback);
```

### Snake_case to camelCase conversion
- Server: `spacetimedb.reducer('do_something', ...)`
- Client: `conn.reducers.doSomething({ ... })`

### Reducer Flags

Control how the server handles reducer calls:

```typescript
// NoSuccessNotify: Don't send TransactionUpdate on success (reduces traffic)
connection.setReducerFlags.movePlayer('NoSuccessNotify');

// FullUpdate: Always send full TransactionUpdate (default)
connection.setReducerFlags.movePlayer('FullUpdate');
```

## Views

Views provide filtered access to private table data based on the connected user.

### ViewContext vs AnonymousViewContext

```typescript
// ViewContext — has ctx.sender, result varies per user (computed per-subscriber)
spacetimedb.view({ name: 'my_items', public: true }, t.array(Item.rowType), (ctx) => {
  return [...ctx.db.item.by_owner.filter(ctx.sender)];
});

// AnonymousViewContext — no ctx.sender, same result for everyone (shared, better perf)
spacetimedb.anonymousView({ name: 'leaderboard', public: true }, t.array(LeaderboardRow), (ctx) => {
  return [...ctx.db.player.by_score.filter(/* top scores */)];
});
```

### CRITICAL: Views can only use index lookups

```typescript
// WRONG — views cannot use .iter()
spacetimedb.view(
  { name: 'my_data_wrong', public: true },
  t.array(PrivateData.rowType),
  (ctx) => [...ctx.db.privateData.iter()]  // NOT ALLOWED
);

// RIGHT — use index lookup
spacetimedb.view(
  { name: 'my_data', public: true },
  t.array(PrivateData.rowType),
  (ctx) => [...ctx.db.privateData.by_owner.filter(ctx.sender)]
);
```

### Subscribing to Views

Views require explicit subscription:

```typescript
conn.subscriptionBuilder().subscribe([
  'SELECT * FROM public_table',
  'SELECT * FROM my_data',  // Views need explicit SQL!
]);
```

## Procedures (Beta)

**Procedures are for side effects (HTTP requests, etc.) that reducers can't do.**

Procedures are currently in beta. API may change.

### Defining a procedure

```typescript
spacetimedb.procedure(
  'fetch_external_data',
  { url: t.string() },
  t.string(),  // return type
  (ctx, { url }) => {
    const response = ctx.http.fetch(url);
    return response.text();
  }
);
```

### CRITICAL: Database access in procedures

**Procedures don't have `ctx.db`. Use `ctx.withTx()` for database access.**

```typescript
spacetimedb.procedure('save_fetched_data', { url: t.string() }, t.unit(), (ctx, { url }) => {
  // Fetch external data (outside transaction)
  const response = ctx.http.fetch(url);
  const data = response.text();

  // WRONG — ctx.db doesn't exist in procedures
  // ctx.db.myTable.insert({ ... });

  // RIGHT — use ctx.withTx() for database access
  ctx.withTx(tx => {
    tx.db.myTable.insert({
      id: 0n,
      content: data,
      fetchedAt: tx.timestamp,
      fetchedBy: tx.sender,
    });
  });

  return {};
});
```

### Key differences from reducers

| Reducers | Procedures |
|----------|------------|
| `ctx.db` available directly | Must use `ctx.withTx(tx => tx.db...)` |
| Automatic transaction | Manual transaction management |
| No HTTP/network | `ctx.http.fetch()` available |
| No return values to caller | Can return data to caller |

## Identity and Authentication

```typescript
import { Identity } from 'spacetimedb';

// Get current identity
const identity = connection.identity;
console.log(identity?.toHexString());

// Compare identities
if (identity?.isEqual(otherIdentity)) {
  console.log('Same user');
}

// Create from hex string
const parsed = Identity.fromString('0x1234...');

// Zero identity
const zero = Identity.zero();

// Compare identities using toHexString()
const isOwner = row.ownerId.toHexString() === myIdentity.toHexString();
```

### Persisting Authentication

```typescript
// On connect, save the token
.onConnect((conn, identity, token) => {
  localStorage.setItem('auth_token', token);
  localStorage.setItem('identity', identity.toHexString());
})

// On reconnect, use saved token
.withToken(localStorage.getItem('auth_token') ?? undefined)
```

### Stale token handling

```typescript
const onConnectError = (_ctx: ErrorContext, err: Error) => {
  if (err.message?.includes('Unauthorized') || err.message?.includes('401')) {
    localStorage.removeItem('auth_token');
    window.location.reload();
  }
};
```

## React Integration

The SDK includes React hooks for reactive UI updates.

### Provider Setup

```tsx
import React, { useMemo } from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection, query } from './module_bindings';
import App from './App';

function Root() {
  // CRITICAL: Memoize to prevent reconnects on every render
  const connectionBuilder = useMemo(() =>
    DbConnection.builder()
      .withUri('ws://localhost:3000')
      .withModuleName('my_game')
      .withToken(localStorage.getItem('auth_token') || undefined)
      .onConnect((conn, identity, token) => {
        console.log('Connected:', identity.toHexString());
        localStorage.setItem('auth_token', token);
        conn.subscriptionBuilder().subscribe(query.player.build());
      })
      .onDisconnect(() => console.log('Disconnected'))
      .onConnectError((ctx, err) => console.error('Error:', err)),
    []  // Empty deps - only create once
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

### useSpacetimeDB Hook

Access connection state:

```tsx
import { useSpacetimeDB } from 'spacetimedb/react';

function ConnectionStatus() {
  const { isActive, identity, token, connectionId, connectionError } = useSpacetimeDB();

  if (connectionError) {
    return <div>Error: {connectionError.message}</div>;
  }

  if (!isActive) {
    return <div>Connecting...</div>;
  }

  return <div>Connected as {identity?.toHexString()}</div>;
}
```

### useTable Hook

Subscribe to table data with reactive updates. **CRITICAL: Returns a tuple `[rows, isLoading]`.**

```tsx
import { useTable, where, eq } from 'spacetimedb/react';
import { tables } from './module_bindings';

function PlayerList() {
  // CORRECT: Tuple destructuring
  const [players, isLoading] = useTable(tables.player);

  if (isLoading) return <div>Loading...</div>;

  return (
    <ul>
      {players.map(player => (
        <li key={player.id}>{player.name}: {player.score}</li>
      ))}
    </ul>
  );
}

function FilteredPlayerList() {
  // Filtered players with callbacks
  const [activePlayers, isLoading] = useTable(
    tables.player,
    where(eq('isActive', true)),
    {
      onInsert: (player) => console.log('Player joined:', player.name),
      onDelete: (player) => console.log('Player left:', player.name),
      onUpdate: (oldPlayer, newPlayer) => {
        console.log(`${oldPlayer.name} updated`);
      },
    }
  );

  return (
    <ul>
      {activePlayers.map(player => (
        <li key={player.id}>{player.name}</li>
      ))}
    </ul>
  );
}
```

### useReducer Hook

Call reducers from components:

```tsx
import { useReducer } from 'spacetimedb/react';
import { reducers } from './module_bindings';

function CreatePlayerForm() {
  const createPlayer = useReducer(reducers.createPlayer);
  const [name, setName] = useState('');

  const handleSubmit = (e) => {
    e.preventDefault();
    // CORRECT: Object syntax
    createPlayer({ name, location: { x: 0, y: 0 } });
    setName('');
  };

  return (
    <form onSubmit={handleSubmit}>
      <input value={name} onChange={e => setName(e.target.value)} />
      <button type="submit">Create Player</button>
    </form>
  );
}
```

## Vue Integration

The SDK includes Vue composables:

```typescript
import { SpacetimeDBProvider, useSpacetimeDB, useTable, useReducer } from 'spacetimedb/vue';
```

Usage is similar to React hooks.

## Svelte Integration

The SDK includes Svelte stores:

```typescript
import { SpacetimeDBProvider, useSpacetimeDB, useTable, useReducer } from 'spacetimedb/svelte';
```

## Server-Side Usage (Node.js, Deno, Bun)

The SDK works in server-side JavaScript runtimes:

```typescript
import { DbConnection } from './module_bindings';

async function main() {
  const connection = DbConnection.builder()
    .withUri('ws://localhost:3000')
    .withModuleName('my_database')
    .onConnect((conn, identity, token) => {
      console.log('Connected:', identity.toHexString());

      conn.subscriptionBuilder()
        .onApplied(() => {
          // Process data
          for (const player of conn.db.player.iter()) {
            console.log(player);
          }
        })
        .subscribe('SELECT * FROM player');
    })
    .build();
}

main();
```

## Timestamps

### Server-side

```typescript
import { Timestamp, ScheduleAt } from 'spacetimedb';

// Current time
ctx.db.item.insert({ id: 0n, createdAt: ctx.timestamp });

// Future time (add microseconds)
const future = ctx.timestamp.microsSinceUnixEpoch + 300_000_000n;  // 5 minutes
```

### Client-side (CRITICAL)

**Timestamps are objects, not numbers:**

```typescript
// WRONG
const date = new Date(row.createdAt);
const date = new Date(Number(row.createdAt / 1000n));

// RIGHT
const date = new Date(Number(row.createdAt.microsSinceUnixEpoch / 1000n));
```

### ScheduleAt on client

```typescript
// ScheduleAt is a tagged union
if (scheduleAt.tag === 'Time') {
  const date = new Date(Number(scheduleAt.value.microsSinceUnixEpoch / 1000n));
}
```

## Scheduled Tables

```typescript
// Scheduled table MUST use scheduledId and scheduledAt columns
export const CleanupJob = table({
  name: 'cleanup_job',
  scheduled: 'run_cleanup'  // reducer name
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  targetId: t.u64(),  // Your custom data
});

// Scheduled reducer receives full row as arg
spacetimedb.reducer('run_cleanup', { arg: CleanupJob.rowType }, (ctx, { arg }) => {
  // arg.scheduledId, arg.targetId available
  // Row is auto-deleted after reducer completes
});

// Schedule a job
import { ScheduleAt } from 'spacetimedb';
const futureTime = ctx.timestamp.microsSinceUnixEpoch + 60_000_000n; // 60 seconds
ctx.db.cleanupJob.insert({
  scheduledId: 0n,
  scheduledAt: ScheduleAt.time(futureTime),
  targetId: someId
});

// Cancel a job by deleting the row
ctx.db.cleanupJob.scheduledId.delete(jobId);
```

## Error Handling

### Connection Errors

```typescript
DbConnection.builder()
  .onConnectError((ctx, error) => {
    console.error('Failed to connect:', error.message);

    // Implement retry logic
    setTimeout(() => {
      // Rebuild connection
    }, 5000);
  })
  .build();
```

### Subscription Errors

```typescript
connection.subscriptionBuilder()
  .onError((ctx, error) => {
    console.error('Subscription failed:', error.message);
  })
  .subscribe('SELECT * FROM player');
```

### Reducer Errors

```typescript
connection.reducers.onCreatePlayer((ctx, args) => {
  const { status } = ctx.event;

  switch (status.tag) {
    case 'Committed':
      console.log('Success');
      break;
    case 'Failed':
      console.error('Reducer failed:', status.value);
      break;
    case 'OutOfEnergy':
      console.error('Out of energy');
      break;
  }
});
```

## Disconnecting

```typescript
// Gracefully disconnect
connection.disconnect();
```

## Type Reference

### Core Types

```typescript
import {
  Identity,           // User identity (256-bit)
  ConnectionId,       // Connection identifier
  Timestamp,          // SpacetimeDB timestamp
  TimeDuration,       // Duration type
  Uuid,               // UUID type
} from 'spacetimedb';
```

### Generated Types

```typescript
// From your module_bindings
import {
  DbConnection,        // Connection class
  DbConnectionBuilder, // Builder class
  SubscriptionBuilder, // Subscription builder
  SubscriptionHandle,  // Subscription handle
  EventContext,        // Event callback context
  ReducerEventContext, // Reducer callback context
  ErrorContext,        // Error callback context
  tables,              // Table accessors for useTable
  reducers,            // Reducer definitions for useReducer
  query,               // Typed query builder

  // Your custom types
  Player,
  Point,
  // ... etc
} from './module_bindings';
```

## Commands

```bash
# Start local server
spacetime start

# Publish module
spacetime publish <module-name> --project-path <backend-dir>

# Clear database and republish
spacetime publish <module-name> --clear-database -y --project-path <backend-dir>

# Generate bindings
spacetime generate --lang typescript --out-dir <client>/src/module_bindings --project-path <backend-dir>

# View logs
spacetime logs <module-name>
```

## Best Practices

1. **Store auth tokens**: Save the token from `onConnect` for seamless reconnection.

2. **Subscribe after connect**: Set up subscriptions in the `onConnect` callback.

3. **Use typed queries**: Prefer the `query` builder over raw SQL strings for type safety.

4. **Handle all connection states**: Implement `onConnect`, `onDisconnect`, and `onConnectError`.

5. **Use light mode for high-frequency updates**: Enable `.withLightMode(true)` for games or real-time apps.

6. **Unsubscribe when done**: Clean up subscriptions when components unmount or data is no longer needed.

7. **Use primary keys**: Define primary keys on tables to enable `onUpdate` callbacks.

8. **Memoize connectionBuilder**: Always wrap in `useMemo()` to prevent reconnects.

9. **Let subscriptions drive state**: Avoid optimistic updates; let the server be the source of truth.

## Common Patterns

### Reconnection Logic

```typescript
function createConnection(token?: string) {
  return DbConnection.builder()
    .withUri('ws://localhost:3000')
    .withModuleName('my_database')
    .withToken(token)
    .onConnect((conn, identity, newToken) => {
      localStorage.setItem('token', newToken);
      setupSubscriptions(conn);
    })
    .onDisconnect(() => {
      // Reconnect after delay
      setTimeout(() => {
        createConnection(localStorage.getItem('token') ?? undefined);
      }, 3000);
    })
    .build();
}
```

### Optimistic Updates

```typescript
function PlayerScore({ player }) {
  const updateScore = useReducer(reducers.updateScore);
  const [optimisticScore, setOptimisticScore] = useState(player.score);

  const handleClick = () => {
    setOptimisticScore(prev => prev + 1);
    updateScore({ playerId: player.id, delta: 1 });
  };

  // Sync with actual data
  useEffect(() => {
    setOptimisticScore(player.score);
  }, [player.score]);

  return <div onClick={handleClick}>Score: {optimisticScore}</div>;
}
```

### Filtering with Multiple Conditions

```typescript
// Using query builder
query.player
  .where(row => row.team.eq('red'))
  .where(row => row.score.gte(100))
  .build();

// Using React hooks
const [redTeamHighScorers] = useTable(
  tables.player,
  where(eq('team', 'red')),  // Additional filtering in client
);
const filtered = redTeamHighScorers.filter(p => p.score >= 100);
```

## Project Structure

### Server (`backend/spacetimedb/`)
```
src/schema.ts   -> Tables, export spacetimedb
src/index.ts    -> Reducers, lifecycle, import schema
package.json    -> { "type": "module", "dependencies": { "spacetimedb": "^1.11.0" } }
tsconfig.json   -> Standard config
```

### Avoiding circular imports
```
schema.ts -> defines tables AND exports spacetimedb
index.ts  -> imports spacetimedb from ./schema, defines reducers
```

### Client (`client/`)
```
src/module_bindings/ -> Generated (spacetime generate)
src/main.tsx         -> Provider, connection setup
src/App.tsx          -> UI components
src/config.ts        -> MODULE_NAME, SPACETIMEDB_URI
```
