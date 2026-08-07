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

**Data policy while building:** the database holds nothing but this app's own
seed data, which your startup seeding recreates — it is disposable. If a schema
change conflicts with existing tables, drop and recreate rather than writing
migration logic.

## Branding & Styling

- App title: **"PostgreSQL <APP_NOUN>"**
- Dark theme using official PostgreSQL brand colors:
  - Primary: `#336791` (PostgreSQL blue)
  - Primary hover: `#008bb9` (lighter PostgreSQL blue)
  - Secondary: `#0064a5` (dark PostgreSQL blue)
  - Background: `#1a1a2e` (dark navy)
  - Surface: `#16213e` (slightly lighter)
  - Border: `#2a2a4a` (muted border)
  - Text: `#e8e8e8` (light gray)
  - Text muted: `#848484` (PostgreSQL light grey)
  - Accent: `#008bb9` (PostgreSQL light blue)
  - Success: `#27ae60` (green for online indicators)
  - Warning: `#f26522` (PostgreSQL light orange)
  - Danger: `#cc3b03` (PostgreSQL dark orange/red)


## Configuration

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| API server port | `<EXPRESS_PORT>` |
| Client dev server | `<VITE_PORT>` |

Use this exact `DATABASE_URL`. Do not point at another PostgreSQL instance and do
not create databases outside it.
