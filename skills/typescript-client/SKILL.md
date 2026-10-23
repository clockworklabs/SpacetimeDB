---
name: typescript-client
description: SpacetimeDB TypeScript/React client SDK reference. Use when building web clients that connect to SpacetimeDB.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: client
  language: typescript
  cursor_globs: "**/*.tsx,**/*.ts"
  cursor_always_apply: true
---

# SpacetimeDB TypeScript Client

Generated bindings convert snake_case names to camelCase, including row fields: a server column `trip_id` is `tripId` on client rows.

## React: main.tsx

```typescript
import React, { useMemo } from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';
import App from './App';

const TOKEN_KEY = `${SPACETIMEDB_URI}/${MODULE_NAME}/auth_token`;

function Root() {
  const connectionBuilder = useMemo(() =>
    DbConnection.builder()
      .withUri(SPACETIMEDB_URI)
      .withDatabaseName(MODULE_NAME)
      .withToken(localStorage.getItem(TOKEN_KEY) ?? undefined)
      .withAutomaticReconnect()
      .onConnect((_conn, _identity, token) => localStorage.setItem(TOKEN_KEY, token)),
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

## React: App.tsx

```typescript
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

export default function App() {
  const { isActive, identity: myIdentity, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  // useTable owns subscriptions and reports readiness after replay.
  const [entities, entitiesReady] = useTable(tables.entity);
  const [records, recordsReady] = useTable(tables.record);

  const [onlineUsers] = useTable(
    tables.entity.where(r => r.active.eq(true)),
    {
      onInsert: user => console.log('User connected:', user.name),
      onDelete: user => console.log('User disconnected:', user.name),
      onUpdate: (oldUser, newUser) => console.log('Updated:', newUser.name),
    }
  );

  const addRecord = (data: string) => {
    if (!conn || !isActive) return;
    void conn.reducers.addRecord({ data }).catch(console.error);
  };
  const ownsEntity = entities.some(
    row => row.owner.toHexString() === myIdentity?.toHexString()
  );

  return (
    <main>
      <p>{isActive ? 'Connected' : 'Not connected'}</p>
      <p>{entitiesReady && recordsReady ? 'Data ready' : 'Waiting for current data'}</p>
      <p>{onlineUsers.length} users online, {records.length} records</p>
      <p>{ownsEntity ? 'You own an entity' : 'No owned entity'}</p>
      <button disabled={!isActive} onClick={() => addRecord('Hello!')}>
        Add record
      </button>
    </main>
  );
}
```

## Vanilla (non-React)

```typescript
import { DbConnection, tables } from './module_bindings';

const HOST = 'wss://maincloud.spacetimedb.com';
const DATABASE = 'my_module';
const TOKEN_KEY = `${HOST}/${DATABASE}/auth_token`;

const conn = DbConnection.builder()
  .withUri(HOST)
  .withDatabaseName(DATABASE)
  .withToken(localStorage.getItem(TOKEN_KEY) ?? undefined)
  .withAutomaticReconnect()
  .onConnect((_conn, identity, token) => {
    localStorage.setItem(TOKEN_KEY, token);
    console.log('Connected as:', identity.toHexString());
  })
  .onDisconnect((_ctx, error, nextAttempt, delayMs) => {
    if (nextAttempt !== undefined) {
      console.warn(`Reconnect attempt ${nextAttempt} in ${delayMs} ms`, error);
    } else {
      console.log('Connection ended', error);
    }
  })
  .onConnectError((_ctx, error, nextAttempt, delayMs) => {
    console.error('Connection failed:', error);
    if (nextAttempt !== undefined) {
      console.log(`Retry ${nextAttempt} in ${delayMs} ms`);
    }
  })
  .build();

// Register once; the SDK replays this subscription after reconnecting.
const subscription = conn.subscriptionBuilder()
  .onApplied(() => console.log('Ready'))
  .subscribe([tables.user, tables.message]);

// Row callbacks
conn.db.user.onInsert((ctx, user) => console.log('Joined:', user.name));
conn.db.user.onDelete((ctx, user) => console.log('Left:', user.name));
conn.db.user.onUpdate((ctx, oldUser, newUser) => console.log('Updated:', newUser.name));
```

## Automatic Reconnect and Token Refresh

The React provider's connection manager enables automatic reconnect; the explicit `.withAutomaticReconnect()` above also shows the setting to use for direct connections. Without it, a direct connection does not recover automatically. For direct connections, initial failures are not retried. After an established connection drops, retries use exponential backoff and jitter from `minDelayMs` (default 1 s) up to `maxDelayMs` (default 30 s), until `disconnect()` or a terminal failure. Tune these with `.withAutomaticReconnect({ minDelayMs, maxDelayMs })`; values below the 500 ms and 1 s floors are raised with a warning, so that retrying clients cannot overwhelm the database. While mounted, the React provider also preserves its separate connection-manager retries: it builds a replacement connection after an initial or terminal failure that the core connection will not retry. It respects an explicit `disconnect()`.

The same connection, identity, table handles, subscriptions, and row callbacks survive recovery. Each attempt gets a fresh connection ID. `onConnect` runs again before subscription replay, so register subscriptions and row callbacks once, outside that callback. The SDK replays subscriptions in one batch, retains readable but stale cached rows during outages, and emits net row changes after reconciliation. Subscription `onApplied` runs again after replay; keep one-time setup separate.

In React, let `useTable` manage its own subscriptions. Do not add a second subscription effect for the same queries or recreate subscriptions whenever `isActive` changes. Use the hook's `isReady` result for data readiness: a successful reconnect handshake does not mean replay has completed. Invoke reducers from event handlers, not during rendering. For a manually created subscription, retain its handle and call `unsubscribe()` when it is no longer needed, including during an outage.

For direct connections, `conn.isReconnecting` reports recovery before the next successful handshake. `onDisconnect` and `onConnectError` receive `(ctx, error, nextReconnectAttempt, nextReconnectDelayMs)`. The last two arguments are `undefined` when no retry is scheduled. Do not build a replacement connection or run your own retry timer while automatic recovery is pending. An explicit `conn.disconnect()` stops recovery, including a pending token refresh result.

For expiring credentials, also call `.withTokenProvider(() => refreshTokenAsync())`, where your authentication integration supplies `refreshTokenAsync(): Promise<string>`. Supply the initial token with `.withToken(initialToken)`; the provider is used only for reconnect attempts. It must return a non-empty token for the same identity. The SDK calls it when remaining validity is at most 30 seconds or 5% of the original lifetime, whichever is greater, when expiry cannot be read, or after a reused token is rejected. Provider failures retry; rejection of a freshly provided token is terminal. No periodic refresh runs while connected, and disconnecting does not cancel the provider's own asynchronous work.

Calls made while disconnected fail immediately. Pending reducer and procedure promises reject with `UnknownCallResultError` (exported from `spacetimedb`) when the connection is lost before a result arrives. The server may have executed the operation; the SDK never replays it. Do not automatically retry non-idempotent calls on that error.

## Gotchas

- **`useTable` rows are `readonly`.** Copy before sorting/mutating, or it fails to type-check:
  `const [rows] = useTable(tables.message); const sorted = [...rows].sort(...)`.
- **bigint in JSX.** ids/counts from `t.u64()`/`t.i64()` columns are `bigint`, which React
  cannot render. Wrap it: `{Number(row.id)}` or `{String(count)}`.
