# Backend: PostgreSQL

An Express API server with Socket.io for live updates, Drizzle ORM over
PostgreSQL, and a React client.

## Layout

```
<app-dir>/
  server/
    package.json          express, socket.io, drizzle-orm, pg, dotenv, tsx
    .env                  DATABASE_URL and PORT
    drizzle.config.ts
    src/schema.ts         Drizzle table definitions
    src/index.ts          Express routes, Socket.io handlers
  client/
    package.json          react, react-dom, vite, socket.io-client
    vite.config.ts        server.port <VITE_PORT>, proxy /api and /socket.io to <EXPRESS_PORT>
    index.html
    src/main.tsx
    src/App.tsx
```

## Deploy

```bash
cd server && npm install && npx drizzle-kit push && npm run dev   # on <EXPRESS_PORT>
cd client && npm install && npm run dev                            # on <VITE_PORT>
```

Re-run `npx drizzle-kit push` after any schema change.

## Identity

The app must show **"PostgreSQL Chat"** as its visible title, in the sidebar or
header, and use it as the page `<title>`. Several backends are benchmarked with
the same specification, so an app that does not name itself is impossible to tell
apart from the others — by a person grading it or by the harness checking it
reached the right server.

## Configuration

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| API server port | `<EXPRESS_PORT>` |
| Client dev server | `<VITE_PORT>` |

Use this exact `DATABASE_URL`. Do not point at another PostgreSQL instance and do
not create databases outside it.
