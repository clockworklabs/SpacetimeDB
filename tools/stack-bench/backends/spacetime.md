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
    src/module_bindings/  generated; never edit by hand
```

## Deploy

Publish the module, then regenerate the client bindings from it:

```bash
<STDB_BIN> publish <MODULE_NAME> --module-path backend/spacetimedb -s <STDB_URI> --yes
<STDB_BIN> generate --lang typescript --out-dir client/src/module_bindings --module-path backend/spacetimedb
```

**While iterating, run development mode instead of republishing by hand.** It
watches the module and automatically rebuilds, publishes, and regenerates the
client bindings on every save:

```bash
<STDB_BIN> dev <MODULE_NAME> --module-path backend/spacetimedb -s <STDB_URI> --yes
```

Leave it running in the background while you work. The manual commands below
are for one-off publishes and for the first deploy.

Republish after any server change, and regenerate after any schema change.

Keep existing application data when you change the schema.

Always use `--yes` for local publish and development commands. It selects the
CLI's non-interactive authentication flow for the target server. Do not pipe
`y` into the command and do not publish anonymously. Use the same local
identity for every publish to the named module.

Then start the client:

```bash
cd client && npm install && npm run dev
```

`<STDB_BIN> logs <MODULE_NAME> -s <STDB_URI>` shows module output, including reducer errors.

To inspect stored data while debugging:

```bash
<STDB_BIN> sql <MODULE_NAME> "SELECT * FROM item LIMIT 5" -s <STDB_URI>
```

## Configuration

| Setting | Value |
|---|---|
| Server URI | `<STDB_URI>` |
| Module name | `<MODULE_NAME>` |
| Client dev server | `<VITE_PORT>` |

The SDK reference for writing modules and clients is in the skill documents
included with these instructions. Follow them for API specifics: import paths,
type builders, accessors and context typing.
