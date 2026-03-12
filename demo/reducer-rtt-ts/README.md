# Reducer RTT Demo (TypeScript)

This demo measures reducer roundtrip latency by:
1. Calling the `add` reducer.
2. Waiting for the resulting reducer event (observed on the inserted row callback).
3. Printing RTT in milliseconds.

## Layout
- `spacetimedb/`: server module
- `src/main.ts`: Node.js RTT client

## Run

1. Start a local SpacetimeDB server (keep this running):

```bash
spacetime start
```

2. Publish the server module:

```bash
cd demo/reducer-rtt-ts/spacetimedb
pnpm install
spacetime publish --server local reducer-rtt-demo
```

3. Run the client:

```bash
cd demo/reducer-rtt-ts
pnpm install
SPACETIMEDB_DB_NAME=reducer-rtt-demo pnpm dev
```

Optional env vars:
- `SPACETIMEDB_HOST` (default: `ws://localhost:3000`)
- `SPACETIMEDB_DB_NAME` (default: `reducer-rtt-demo`)
- `RTT_INTERVAL_MS` (default: `2000`)
