# Backend: MongoDB

Instructions for generating, building, and deploying the **MongoDB** backend.

This backend uses standard Node.js/TypeScript patterns — you only need this file from `backends/`.

---

## Architecture

- **Server:** Node.js + Express + Mongoose (ODM) + Socket.io
- **Client:** React + Vite + TypeScript + Socket.io-client
- **Database:** MongoDB (running in Docker)

The server handles:
- REST API endpoints for CRUD operations
- Socket.io for real-time events (messages, typing, presence, etc.)
- Mongoose models/queries for the database
- Session/identity management

**Real-time:** Use Socket.io to broadcast changes (messages, typing, presence) to
connected clients. Do NOT use MongoDB change streams — the database runs as a single
node, and the real-time layer is the application's responsibility (same model as a
standard MERN-stack app).

---

## MongoDB Connection

MongoDB is already running in a Docker container.

| Parameter | Value |
|-----------|-------|
| Host | `localhost` |
| Port | `6437` (mapped from container 27017) |
| Database | `chat-app` |
| Container | `llm-sequential-upgrade-mongodb-1` |
| Connection URL | `mongodb://localhost:6437/chat-app` |

---

## Pre-flight Check

```bash
docker exec llm-sequential-upgrade-mongodb-1 mongosh --quiet --eval "db.runCommand({ping:1})"
```

If MongoDB is not reachable, STOP and report the error.

---

## Directory Structure

```
<app-dir>/
  server/
    package.json
    tsconfig.json
    .env
    src/
      models.ts      # Mongoose schema/model definitions
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
      "mongoose": "^8.9.0",
      "socket.io": "^4.7.4",
      "cors": "^2.8.5",
      "@types/cors": "^2.8.17",
      "dotenv": "^16.4.5",
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
  DATABASE_URL=mongodb://localhost:6437/chat-app
  PORT=6001
  ```

- `server/src/models.ts` — Mongoose schemas/models for all features
- `server/src/index.ts` — Express server with:
  - CORS configured for `http://localhost:6373`
  - Socket.io with CORS
  - REST endpoints for the app's resources (per the feature spec)
  - Socket.io events for real-time updates (per the feature spec)
  - Database access via Mongoose (`mongoose.connect(process.env.DATABASE_URL)`)

Install:
```bash
cd <server-dir> && npm install
```

MongoDB is schemaless and Mongoose creates collections/indexes on first use — there
is **no migration / schema-push step**. (If you declare indexes on a schema, they are
built automatically when the model is first used.)

---

## Phase 2: (No bindings step)

Skip — MongoDB has no binding generation. The client calls REST/Socket.io APIs directly.

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

- `client/vite.config.ts` — port **6373** (do not use 6173 or 6273 — they may be in use), proxy `/api` and `/socket.io` to `http://localhost:6001`
  ```typescript
  import { defineConfig } from 'vite';
  import react from '@vitejs/plugin-react';

  export default defineConfig({
    plugins: [react()],
    server: {
      port: 6373,
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

**Critical:** Initialize the socket.io client without a hardcoded URL so it routes through the Vite proxy (e.g. `io()` or `io({ path: '/socket.io' })`). Hardcoding `http://localhost:6001` bypasses the proxy and breaks WebSocket upgrades.

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
npx kill-port 6373 2>/dev/null || true
npx kill-port 6001 2>/dev/null || true

# Start the API server in background
cd <server-dir> && npx tsx src/index.ts &

# Wait for API server to be ready (poll http://localhost:6001 up to 30s)

# Start client dev server in background
cd <client-dir> && npm run dev &
```

Wait for both servers to be ready:
- API server at `http://localhost:6001`
- Client dev server at `http://localhost:6373`

---

## Redeploy (for fix iterations)

- If **server changed**: kill and restart the Express server
  ```bash
  npx kill-port 6001 2>/dev/null || true
  cd <server-dir> && npx tsx src/index.ts &
  ```
- If **models/schema changed**: no migration step — Mongoose applies the new schema
  on connect (existing documents are not rewritten). Just restart the Express server.
- If **client changed**: Vite HMR handles it automatically (or restart dev server if needed)

---

## App Identity

- HTML `<title>` MUST be **"MongoDB Chat"** (not a generic "Chat App")
- The app MUST show **"MongoDB Chat"** as the visible header/title in the UI

---

## Port Configuration

| Service | Port | Notes |
|---------|------|-------|
| MongoDB (Docker) | 6437 | Database |
| Express API server | 6001 | REST + Socket.io |
| Vite dev server | **6373** | React client — do not use 6173 or 6273 |

---

## Reference Files

The language and feature prompt files are provided as absolute paths in the launch prompt. No additional reference files are needed — this backend uses standard Node.js/TypeScript patterns.
