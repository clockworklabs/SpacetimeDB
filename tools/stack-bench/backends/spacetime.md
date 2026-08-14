# Backend: SpacetimeDB

The database runs your server logic. There is no separate API server and no ORM:
tables and reducers are a WASM module you publish, and the client subscribes to
tables and calls reducers over a live connection.

## Layout

```
<app-dir>/
  backend/spacetimedb/
    package.json          { "type": "module", dependencies: { "spacetimedb": "<STDB_PACKAGE>" },
                            devDependencies: { "typescript": "~5.6.2" } }  ← required; the build runs tsc from node_modules
    tsconfig.json
    src/schema.ts         tables and indexes
    src/index.ts          reducers and lifecycle hooks
  client/
    package.json          react, react-dom, vite, and "spacetimedb": "<STDB_PACKAGE>"
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
<STDB_BIN> publish <MODULE_NAME> --module-path backend/spacetimedb -s <STDB_URI> --yes
<STDB_BIN> generate --lang typescript --out-dir client/src/module_bindings --module-path backend/spacetimedb
```

**While iterating, run development mode instead of republishing by hand.** It
watches the module and auto-rebuilds, auto-publishes and auto-regenerates the
client bindings on every save — the equivalent of a watching dev server:

```bash
<STDB_BIN> dev <MODULE_NAME> --module-path backend/spacetimedb -s <STDB_URI> --yes
```

Leave it running in the background while you work. The manual commands below
are for one-off publishes and for the first deploy.

Republish after any server change, and regenerate after any schema change.

**Data policy while building:** the database holds nothing but this app's own
seed data, which your init logic recreates — it is disposable. If a schema
change is rejected as incompatible, do not write migration logic; republish
with `--delete-data` and move on:

```bash
<STDB_BIN> publish <MODULE_NAME> --module-path backend/spacetimedb -s <STDB_URI> --delete-data --yes
```

Always use `--yes` for local publish and development commands. It selects the
CLI's non-interactive authentication flow for the target server as well as
confirming destructive schema changes. Do not pipe `y` into the command and do
not publish anonymously: the benchmark must retain one local identity so the
harness can republish and reset the same named database later in the run.

Then start the client:

```bash
cd client && npm install && npm run dev
```

`<STDB_BIN> logs <MODULE_NAME> -s <STDB_URI>` shows module output, including reducer errors.

To inspect stored data while debugging:

```bash
<STDB_BIN> sql <MODULE_NAME> "SELECT * FROM item LIMIT 5" -s <STDB_URI>
```

## Branding & Styling

- App title: **"SpacetimeDB <APP_NOUN>"**
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
| Server URI | `<STDB_URI>` |
| Module name | `<MODULE_NAME>` |
| Client dev server | `<VITE_PORT>` |

The SDK reference for writing modules and clients is in the skill documents
included with these instructions. Follow them for API specifics — import paths,
type builders, accessors and context typing.
