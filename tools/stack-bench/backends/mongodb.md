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

## Branding & Styling

- App title: **"MongoDB <APP_NOUN>"**
- Dark theme using official MongoDB brand colors:
  - Primary: `#00ED64` (MongoDB green)
  - Primary hover: `#00C957` (darker green)
  - Secondary: `#00684A` (MongoDB forest green)
  - Background: `#001E2B` (MongoDB dark slate)
  - Surface: `#023430` (deep green-slate)
  - Border: `#1C2D38` (muted slate border)
  - Text: `#E8EDEB` (light gray)
  - Text muted: `#889397` (MongoDB gray)
  - Accent: `#00ED64` (MongoDB green)
  - Success: `#00ED64` (green for online indicators)
  - Warning: `#FFC010` (MongoDB amber)
  - Danger: `#FF4F4F` (MongoDB red)


## Configuration

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| API server port | `<EXPRESS_PORT>` |
| Client dev server | `<VITE_PORT>` |

Use this exact `DATABASE_URL`. Do not point at another MongoDB instance.
