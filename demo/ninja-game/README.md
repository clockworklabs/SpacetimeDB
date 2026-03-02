# Ninja Game (Swift + SpacetimeDB)

Realtime 2D multiplayer demo used to validate the Swift SDK at game-like update rates.

## Local Setup

From repo root:

```bash
spacetime start
spacetime publish -s local -p demo/ninja-game/spacetimedb ninjagame -c -y
```

Run the client:

```bash
cd demo/ninja-game/client-swift
open Package.swift
```

In Xcode, run `NinjaGameClient`.

## Multi-Client Soak Check (Milestone 6)

1. Launch two client instances (or run once in Xcode and once with `swift run`).
2. Join both clients with different names.
3. Move on both clients for 2-3 minutes.
4. Trigger combat, pickups, respawn, and clear-server flow.

Expected signals:

- Status bar remains `Connected`.
- Player count and positions replicate across both clients in near real time.
- Weapon drops and pickups replicate consistently.
- Combat health/kills updates match across clients.
- No repeated reducer failures in logs.

## Troubleshooting

- `missing reducer ... publish ninjagame module` in status bar:
  module on server is stale; re-run publish command and reconnect.
- `Disconnected` or websocket errors:
  confirm local server is running on `http://127.0.0.1:3000`.
- No replicated updates:
  verify `ninjagame` database name and that the module publish succeeded.
