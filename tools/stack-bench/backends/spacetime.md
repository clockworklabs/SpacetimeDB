# Backend: SpacetimeDB

The database runs your server logic. There is no separate API server and no ORM:
tables and reducers are a WASM module you publish, and the client subscribes to
tables and calls reducers over a live connection.

## Layout

```
<app-dir>/
  backend/spacetimedb/
    package.json          { "type": "module", dependencies: { "spacetimedb": "^2.0.0" } }
    tsconfig.json
    src/schema.ts         tables and indexes
    src/index.ts          reducers and lifecycle hooks
  client/
    package.json          react, react-dom, vite, spacetimedb
    vite.config.ts        server.port must be <VITE_PORT>
    index.html
    src/config.ts         MODULE_NAME and SPACETIMEDB_URI
    src/main.tsx          React entry
    src/App.tsx
    src/module_bindings/  generated — never edit by hand
```

## Deploy

Publish the module, then regenerate the client bindings from it:

```bash
spacetime publish <MODULE_NAME> --module-path backend/spacetimedb
spacetime generate --lang typescript --out-dir client/src/module_bindings --module-path backend/spacetimedb
```

Republish after any server change, and regenerate after any schema change.
If a schema change is rejected as incompatible, republish with `--delete-data`:

```bash
echo y | spacetime publish <MODULE_NAME> --module-path backend/spacetimedb --delete-data
```

Then start the client:

```bash
cd client && npm install && npm run dev
```

`spacetime logs <MODULE_NAME>` shows module output, including reducer errors.

## Branding & Styling

- App title: **"SpacetimeDB Chat"**
- Dark theme using official SpacetimeDB brand colors:
  - Primary: `#4cf490` (SpacetimeDB green)
  - Primary hover: `#4cf490bf` (green 75% opacity)
  - Secondary: `#a880ff` (SpacetimeDB purple)
  - Background: `#0d0d0e` (shade2 — near black)
  - Surface: `#141416` (shade1 — slightly lighter)
  - Border: `#202126` (n6)
  - Text: `#e6e9f0` (n1 — light gray)
  - Text muted: `#6f7987` (n4)
  - Accent: `#02befa` (SpacetimeDB blue)
  - Success: `#4cf490` (green — same as primary)
  - Warning: `#fbdc8e` (SpacetimeDB yellow)
  - Danger: `#ff4c4c` (SpacetimeDB red)
  - Gradient (optional, for headers): `linear-gradient(266deg, #4cf490 0%, #8a38f5 100%)` (green to purple)


## Configuration

| Setting | Value |
|---|---|
| Server URI | `http://localhost:3000` |
| Module name | `<MODULE_NAME>` |
| Client dev server | `<VITE_PORT>` |

The SDK reference for writing modules and clients is in the skill documents
included with these instructions. Follow them for API specifics — import paths,
type builders, accessors and context typing.
