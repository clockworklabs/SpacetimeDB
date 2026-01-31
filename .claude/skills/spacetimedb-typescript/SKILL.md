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

Reducers are server-side functions that modify the database:

```typescript
// Call a reducer
connection.reducers.createPlayer({ name: 'Alice', location: { x: 0, y: 0 } });

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

### Reducer Flags

Control how the server handles reducer calls:

```typescript
// NoSuccessNotify: Don't send TransactionUpdate on success (reduces traffic)
connection.setReducerFlags.movePlayer('NoSuccessNotify');

// FullUpdate: Always send full TransactionUpdate (default)
connection.setReducerFlags.movePlayer('FullUpdate');
```

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

## React Integration

The SDK includes React hooks for reactive UI updates.

### Provider Setup

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection, query } from './module_bindings';
import App from './App';

const connectionBuilder = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('my_game')
  .onConnect((conn, identity, token) => {
    console.log('Connected:', identity.toHexString());
    conn.subscriptionBuilder().subscribe(query.player.build());
  })
  .onDisconnect(() => console.log('Disconnected'))
  .onConnectError((ctx, err) => console.error('Error:', err));

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
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

Subscribe to table data with reactive updates:

```tsx
import { useTable, where, eq } from 'spacetimedb/react';
import { tables } from './module_bindings';

function PlayerList() {
  // All players
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

## Best Practices

1. **Store auth tokens**: Save the token from `onConnect` for seamless reconnection.

2. **Subscribe after connect**: Set up subscriptions in the `onConnect` callback.

3. **Use typed queries**: Prefer the `query` builder over raw SQL strings for type safety.

4. **Handle all connection states**: Implement `onConnect`, `onDisconnect`, and `onConnectError`.

5. **Use light mode for high-frequency updates**: Enable `.withLightMode(true)` for games or real-time apps.

6. **Unsubscribe when done**: Clean up subscriptions when components unmount or data is no longer needed.

7. **Use primary keys**: Define primary keys on tables to enable `onUpdate` callbacks.

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
