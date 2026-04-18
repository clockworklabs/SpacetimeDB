<!-- run-id: spacetime-level1-20260406-153727 -->

# Backend: SpacetimeDB

Instructions for generating, building, and deploying the **SpacetimeDB** backend.

---

## Pre-flight Check

```bash
spacetime server ping local
```

If SpacetimeDB is not running, STOP and report the error.

---

## Directory Structure

```
<app-dir>/
  backend/spacetimedb/
    package.json
    tsconfig.json
    src/
      schema.ts      # All tables and indexes
      index.ts       # All reducers and lifecycle hooks
  client/
    package.json
    vite.config.ts
    tsconfig.json
    index.html
    src/
      config.ts      # Module name and SpacetimeDB URI
      main.tsx       # React entry point
      App.tsx        # Main application component
      styles.css     # Dark theme styling
      module_bindings/  # Auto-generated (Phase 2)
```

---

## Phase 1: Generate Backend

- Create `backend/spacetimedb/package.json` (use template in "Backend Templates" section below)
- Create `backend/spacetimedb/tsconfig.json` (use template below)
- Create `backend/spacetimedb/src/schema.ts` — all tables and indexes
- Create `backend/spacetimedb/src/index.ts` — all reducers and lifecycle hooks
- Install and publish:
  ```bash
  cd <backend-dir> && npm install
  spacetime publish chat-app-<timestamp> --module-path <backend-dir>
  ```

**Module naming:** Use the timestamped folder name as the module name (e.g. `chat-app-20260330-143000`).

---

## Phase 2: Generate Bindings

```bash
spacetime generate --lang typescript --out-dir <client>/src/module_bindings --module-path <backend-dir>
```

Read the generated bindings to know the exact type names (table names, reducer signatures) before writing client code.

---

## Phase 3: Generate Client

Generate client files using the REAL binding types from Phase 2.

- Create `client/package.json` (use template below)
- Create `client/vite.config.ts` (use template below)
- Create `client/tsconfig.json` (use template below)
- Create `client/index.html` (use template below)
- Create `client/src/config.ts` — module name and SpacetimeDB URI
- Create `client/src/main.tsx` — React entry point
- Create `client/src/App.tsx` — main application component
- Create `client/src/styles.css` — dark theme styling

**CRITICAL:** Import from `./module_bindings` using the REAL generated type names, not guessed ones.

---

## Phase 4: Verify

```bash
cd <client-dir> && npm install
npx tsc --noEmit          # Type-check
npm run build             # Full production build
```

Both must pass. If either fails:
1. Read the error
2. Fix the code
3. Retry (up to 3 attempts)
4. Each fix counts as a **reprompt** — log it

---

## Phase 5: Deploy

```bash
# Kill any existing dev server
npx kill-port 6173 2>/dev/null || true

# Start dev server in background
cd <client-dir> && npm run dev &
```

Wait for the dev server to be ready (poll `http://localhost:6173` up to 30 seconds).

---

## App Identity

- HTML `<title>` MUST be **"SpacetimeDB Chat"** (not "Chat App" or anything generic)
- The app MUST show **"SpacetimeDB Chat"** as the visible header/title in the UI
- This distinguishes it from the PostgreSQL version during testing

---

## Redeploy (for fix iterations)

- If **backend changed**: re-publish module, regenerate bindings if schema changed
  ```bash
  spacetime publish chat-app-<timestamp> --module-path <backend-dir>
  spacetime generate --lang typescript --out-dir <client>/src/module_bindings --module-path <backend-dir>
  ```
- If **client changed**: Vite HMR handles it automatically (or restart dev server if needed)

---

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

---

# SpacetimeDB File Templates

## Backend Templates

### backend/spacetimedb/package.json
```json
{
  "name": "chat-app-backend",
  "type": "module",
  "version": "1.0.0",
  "dependencies": {
    "spacetimedb": "^2.0.0"
  }
}
```

### backend/spacetimedb/tsconfig.json
```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "node",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "outDir": "./dist"
  },
  "include": ["src/**/*"]
}
```

### File Organization
```
src/schema.ts   -> All tables, indexes, export spacetimedb
src/index.ts    -> Import schema, define all reducers and lifecycle hooks
```

Why this structure? Avoids circular dependency issues between tables and reducers.

---

## Client Templates

### client/package.json
```json
{
  "name": "chat-app-client",
  "private": true,
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "kill-port": "npx kill-port 6173 2>nul || true",
    "dev": "npm run kill-port && vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "spacetimedb": "^2.0.0"
  },
  "devDependencies": {
    "@types/react": "^18.3.18",
    "@types/react-dom": "^18.3.5",
    "@vitejs/plugin-react": "^4.3.4",
    "typescript": "^5.7.2",
    "vite": "^6.0.3"
  }
}
```

### client/vite.config.ts
```typescript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 6173,  // NEVER use 3000 — conflicts with SpacetimeDB
  },
});
```

### client/tsconfig.json
```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"]
}
```

### client/index.html
```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>SpacetimeDB Chat</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

### client/src/config.ts
```typescript
export const MODULE_NAME = 'chat-app-TIMESTAMP';  // Replace TIMESTAMP with actual module name
export const SPACETIMEDB_URI = 'ws://localhost:3000';
```

---

## Port Configuration

| Service | Port | Notes |
|---------|------|-------|
| SpacetimeDB server | 3000 | WebSocket connections |
| Vite dev server | 6173 | React client |

**Never run Vite on port 3000** — it conflicts with SpacetimeDB.
