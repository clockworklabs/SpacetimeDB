# Blackholio Phaser Client

Browser client for the existing Blackholio Rust SpacetimeDB module, built with
TypeScript and Phaser.

## Requirements

- Node.js 18 or later
- pnpm 10.16.0 or later
- A locally available SpacetimeDB CLI/server

This is a standalone pnpm project. Its local `pnpm-workspace.yaml` prevents
pnpm from selecting the repository workspace, and its local dependency
configuration enforces a 1440-minute minimum release age.

The checked-in dependency configuration links the TypeScript SDK from this
repository during development:

```json
"spacetimedb": "link:../../../crates/bindings-typescript"
```

When publishing this demo outside the SpacetimeDB repository, replace the local
link with the current published npm package version:

```json
"spacetimedb": "<latest-published-version>"
```

`package.json` does not support commented dependency alternatives, so the
published form is documented here rather than left as an invalid commented
entry in the manifest.

## Run Locally

Start the existing Rust server using its normal development workflow, then
from this directory:

```bash
pnpm install
pnpm dev
```

To regenerate this client's checked-in TypeScript bindings without changing
the server project:

```bash
spacetime generate --lang typescript --out-dir src/module_bindings --module-path ../server-rust
```

## Source Layout

The client follows the same controller boundaries as the Unity example:

- `GameManager.ts`: SpacetimeDB connection, subscriptions, and entity/player registries
- `PlayerController.ts`: local input and owned-circle state
- `EntityController.ts`, `CircleController.ts`, and `FoodController.ts`: rendered entities
- `CameraController.ts`: center-of-mass following and zoom
- `ui/`: username chooser, death screen, leaderboard, and browser HUD

The client connects to `ws://localhost:3000` and database `blackholio` by
default. Override either value when starting Vite:

```bash
VITE_SPACETIMEDB_HOST=ws://localhost:3000 \
VITE_SPACETIMEDB_DB_NAME=blackholio \
pnpm dev
```

## Controls

- Pointer: steer
- `Space`: split
- `Q`: lock or unlock steering direction
- `S`: self-destruct
