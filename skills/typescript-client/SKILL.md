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

## React: main.tsx

```typescript
import React, { useEffect, useMemo } from 'react';
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

## React: App.tsx

```typescript
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  // Save auth token
  useEffect(() => { if (token) localStorage.setItem('auth_token', token); }, [token]);

  // Subscribe when connected. Prefer typed query builders over raw SQL
  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([tables.entity, tables.record]);
      // Or with filters: tables.entity.where(r => r.active.eq(true))
      // Or raw SQL:      'SELECT * FROM entity'
  }, [conn, isActive]);

  // Reactive data. Returns [rows, isReady]
  const [entities, entitiesReady] = useTable(tables.entity);
  const [records, recordsReady] = useTable(tables.record);

  // useTable with row callbacks
  const [onlineUsers] = useTable(
    tables.entity.where(r => r.active.eq(true)),
    {
      onInsert: (user) => console.log('User connected:', user.name),
      onDelete: (user) => console.log('User disconnected:', user.name),
      onUpdate: (oldUser, newUser) => console.log('Updated:', newUser.name),
    }
  );

  // Call reducers with object syntax
  conn?.reducers.addRecord({ data });

  // Compare identities
  const isMe = row.owner.toHexString() === myIdentity?.toHexString();
}
```

## Vanilla (non-React)

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('wss://maincloud.spacetimedb.com')
  .withDatabaseName('my_module')
  .onConnect((ctx) => {
    ctx.subscriptionBuilder()
      .onApplied(() => console.log('Ready'))
      .subscribe([tables.user, tables.message]);
  })
  .build();

// Row callbacks
conn.db.user.onInsert((ctx, user) => console.log('Joined:', user.name));
conn.db.user.onDelete((ctx, user) => console.log('Left:', user.name));
conn.db.user.onUpdate((ctx, oldUser, newUser) => console.log('Updated:', newUser.name));
```
