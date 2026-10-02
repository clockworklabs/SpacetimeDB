# Starclash lobby example

A one-on-one spaceship game. Find an opponent, choose maneuvers, and finish
a duel to update your rating. You can also play against the computer.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-lobby-ts/example`, copy [.env.example](./.env.example) to `.env`.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8797>.

## Try it

1. Open the app in a normal window and a private browser window.
2. Choose a pilot name and ship in each window.
3. Click **Find Match** in both windows to queue the two pilots.
4. Choose a maneuver for each pilot. The round resolves after both choose.
5. Continue until one ship is destroyed, then check the ratings.

Use separate browser profiles or a private window. Two normal tabs share the
same pilot identity.

If no opponent joins, the app starts a duel against the computer.
Computer duels do not affect ratings.

## Leaving a duel

**Forfeit** ends an active duel as a loss. Starting another duel also forfeits
the current one. If your last connection closes and you do not reconnect within
30 seconds, you forfeit. The opponent wins a rated active duel in these cases.

## Configuration

See [.env.example](./.env.example) for the database address and web-server port.

## Before deploying

The demo identifies pilots by browser identity. Add accounts if players need
account recovery or lasting competitive profiles.

## Troubleshooting

- **Pilots do not match:** queue both before the computer fallback starts.
- **Ratings do not change:** computer duels are unrated.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): matchmaking, combat, and results.
- [spacetimedb/src/catalog.ts](./spacetimedb/src/catalog.ts): ships and maneuvers.
- [src/app.ts](./src/app.ts): pilot controls and duel display.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-lobby-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
