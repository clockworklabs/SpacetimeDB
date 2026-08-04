# Backend: MongoDB

An Express API server with Socket.io for live updates, Mongoose over MongoDB,
and a React client.

## Layout

```
<app-dir>/
  server/
    package.json          express, socket.io, mongoose, dotenv, tsx
    .env                  DATABASE_URL and PORT
    src/models.ts         Mongoose schemas and models
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
cd server && npm install && npm run dev    # on <EXPRESS_PORT>
cd client && npm install && npm run dev    # on <VITE_PORT>
```

Mongoose creates collections on first write; there is no migration step.

## Configuration

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| API server port | `<EXPRESS_PORT>` |
| Client dev server | `<VITE_PORT>` |

Use this exact `DATABASE_URL`. Do not point at another MongoDB instance.
