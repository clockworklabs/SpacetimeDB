# Backend: PostgreSQL

Instructions for generating, building, and deploying the **PostgreSQL** backend.

**Do NOT read SpacetimeDB SDK rule files.** This backend uses standard Node.js/TypeScript patterns.

---

## Architecture

- **Server:** Node.js + Express + Drizzle ORM + Socket.io
- **Client:** React + Vite + TypeScript + Socket.io-client
- **Database:** PostgreSQL (running in Docker)

The server handles:
- REST API endpoints for CRUD operations
- Socket.io for real-time events (messages, typing, presence, etc.)
- Drizzle ORM for database queries
- Session/identity management

---

## PostgreSQL Connection

PostgreSQL is already running in a Docker container.

| Parameter | Value |
|-----------|-------|
| Host | `localhost` |
| Port | `6432` (mapped from container 5432) |
| User | `spacetime` |
| Password | `spacetime` |
| Database | `spacetime` |
| Container | `spacetime-web-postgres-1` |
| Connection URL | `postgresql://spacetime:spacetime@localhost:6432/spacetime` |

---

## Pre-flight Check

```bash
docker exec spacetime-web-postgres-1 psql -U spacetime -d spacetime -c "SELECT 1"
```

If PostgreSQL is not reachable, STOP and report the error.

---

## Directory Structure

```
<app-dir>/
  server/
    package.json
    tsconfig.json
    drizzle.config.ts
    .env
    src/
      schema.ts      # Drizzle ORM table definitions
      index.ts       # Express server + Socket.io + routes
  client/
    package.json
    vite.config.ts
    tsconfig.json
    index.html
    src/
      main.tsx       # React entry point
      App.tsx        # Main application component
      styles.css     # Dark theme styling
```

---

## Phase 1: Generate Server

Create the Express + Socket.io server:

- `server/package.json`:
  ```json
  {
    "name": "chat-server",
    "type": "module",
    "scripts": {
      "dev": "tsx watch src/index.ts",
      "start": "tsx src/index.ts"
    },
    "dependencies": {
      "express": "^4.18.2",
      "@types/express": "^4.17.21",
      "drizzle-orm": "^0.39.0",
      "pg": "^8.13.0",
      "@types/pg": "^8.11.0",
      "socket.io": "^4.7.4",
      "cors": "^2.8.5",
      "@types/cors": "^2.8.17",
      "dotenv": "^16.4.5",
      "drizzle-kit": "^0.30.0",
      "tsx": "^4.19.0",
      "typescript": "^5.4.0"
    }
  }
  ```

- `server/tsconfig.json`:
  ```json
  {
    "compilerOptions": {
      "target": "ES2022",
      "module": "ES2022",
      "moduleResolution": "bundler",
      "esModuleInterop": true,
      "strict": true,
      "outDir": "dist",
      "rootDir": "src",
      "skipLibCheck": true
    },
    "include": ["src/**/*"]
  }
  ```

- `server/.env`:
  ```
  DATABASE_URL=postgresql://spacetime:spacetime@localhost:6432/spacetime
  PORT=3001
  ```

- `server/drizzle.config.ts`:
  ```typescript
  import { defineConfig } from 'drizzle-kit';

  export default defineConfig({
    schema: './src/schema.ts',
    out: './drizzle',
    dialect: 'postgresql',
    dbCredentials: {
      url: process.env.DATABASE_URL || 'postgresql://spacetime:spacetime@localhost:6432/spacetime',
    },
  });
  ```

- `server/src/schema.ts` — Drizzle ORM table definitions for all features
- `server/src/index.ts` — Express server with:
  - CORS configured for `http://localhost:6273`
  - Socket.io with CORS
  - REST endpoints for rooms, messages, users
  - Socket.io events for real-time: typing, messages, presence, read receipts
  - Database queries via Drizzle ORM

Install and push schema:
```bash
cd <server-dir> && npm install
npx drizzle-kit push
```

---

## Phase 2: (No bindings step)

Skip — PostgreSQL has no binding generation. The client calls REST/Socket.io APIs directly.

---

## Phase 3: Generate Client

- `client/package.json`:
  ```json
  {
    "name": "chat-client",
    "type": "module",
    "scripts": {
      "dev": "vite",
      "build": "tsc -b && vite build"
    },
    "dependencies": {
      "react": "^18.3.1",
      "react-dom": "^18.3.1",
      "socket.io-client": "^4.7.4"
    },
    "devDependencies": {
      "@types/react": "^18.3.12",
      "@types/react-dom": "^18.3.1",
      "@vitejs/plugin-react": "^4.3.4",
      "typescript": "^5.4.0",
      "vite": "^6.0.0"
    }
  }
  ```

- `client/vite.config.ts` — port **6273** (NOT 6173 — that's SpacetimeDB), proxy `/api` and `/socket.io` to `http://localhost:6001`
  ```typescript
  import { defineConfig } from 'vite';
  import react from '@vitejs/plugin-react';

  export default defineConfig({
    plugins: [react()],
    server: {
      port: 6273,
      proxy: {
        '/api': 'http://localhost:6001',
        '/socket.io': {
          target: 'http://localhost:6001',
          ws: true,
        },
      },
    },
  });
  ```

- `client/tsconfig.json`
- `client/index.html`
- `client/src/main.tsx` — React entry point
- `client/src/App.tsx` — Main component using `fetch('/api/...')` + Socket.io client
- `client/src/styles.css` — Dark theme styling

**The client connects to the server via the Vite proxy** — no hardcoded localhost:6001 in client code.

---

## Phase 4: Verify

```bash
# Server
cd <server-dir> && npm install && npx tsc --noEmit

# Client
cd <client-dir> && npm install && npx tsc --noEmit && npm run build
```

Both must pass. If either fails:
1. Read the error
2. Fix the code
3. Retry (up to 3 attempts)
4. Each fix counts as a **reprompt** — log it

---

## Phase 5: Deploy

```bash
# Kill any existing servers
npx kill-port 6273 2>/dev/null || true
npx kill-port 3001 2>/dev/null || true

# Start the API server in background
cd <server-dir> && npx tsx src/index.ts &

# Wait for API server to be ready (poll http://localhost:6001 up to 30s)

# Start client dev server in background
cd <client-dir> && npm run dev &
```

Wait for both servers to be ready:
- API server at `http://localhost:6001`
- Client dev server at `http://localhost:6273`

---

## Redeploy (for fix iterations)

- If **server changed**: kill and restart the Express server
  ```bash
  npx kill-port 3001 2>/dev/null || true
  cd <server-dir> && npx tsx src/index.ts &
  ```
- If **schema changed**: push new schema before restarting
  ```bash
  cd <server-dir> && npx drizzle-kit push
  ```
- If **client changed**: Vite HMR handles it automatically (or restart dev server if needed)

---

## Key Differences from SpacetimeDB

For context on what makes this backend different (this helps the benchmark comparison):

| Aspect | SpacetimeDB | PostgreSQL |
|--------|-------------|------------|
| Real-time | Built-in subscriptions | Socket.io (manual) |
| API layer | Reducers (auto-exposed) | Express routes (manual) |
| Schema | `table()` + `reducer()` | Drizzle `pgTable()` |
| Bindings | Auto-generated types | Manual type definitions |
| Deployment | `spacetime publish` | Start Express server |
| State sync | Automatic client cache | Manual fetch + Socket.io |
| Online presence | Via lifecycle hooks | Manual Socket.io tracking |
| Typing indicators | Reducer + subscription | Socket.io events |
| Infra dependencies | SpacetimeDB only | PostgreSQL + Express + Socket.io + CORS |

---

## App Identity

- HTML `<title>` MUST be **"PostgreSQL Chat"** (not "Chat App", not "SpacetimeDB Chat")
- The app MUST show **"PostgreSQL Chat"** as the visible header/title in the UI
- This distinguishes it from the SpacetimeDB version during testing

---

## Port Configuration

| Service | Port | Notes |
|---------|------|-------|
| PostgreSQL (Docker) | 6432 | Database |
| Express API server | 3001 | REST + Socket.io |
| Vite dev server | **6273** | React client — NOT 6173 (that's SpacetimeDB) |

---

## Reference Files

The language and feature prompt files are provided as absolute paths in the launch prompt. No additional reference files are needed — this backend uses standard Node.js/TypeScript patterns.
