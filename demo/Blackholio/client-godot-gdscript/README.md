# Blackholio — Godot (GDScript) client

A [Godot 4.7](https://godotengine.org) port of the Blackholio demo (agar.io-style),
written in **GDScript**. It complements the existing C# client at
[`../client-godot`](../client-godot): same game, same server, no .NET toolchain
required.

The client is built on the [SpacetimeDB Godot SDK](https://github.com/plaught-armor/Godot-SpacetimeDB-SDK)
(a maintained fork of [flametime/Godot-SpacetimeDB-SDK](https://github.com/flametime/Godot-SpacetimeDB-SDK),
the SDK used by the original Godot PR #3128). The SDK addon is vendored under
`addons/SpacetimeDB/` (MIT) so the project runs without extra install steps.

## What it demonstrates

- Connect over the v3 WebSocket/BSATN protocol → subscribe → receive the initial
  snapshot and incremental row updates.
- Full gameplay loop: enter game, move, split, suicide, respawn.
- Entity / food / circle rendering, leaderboard, follow camera, reconnect rejoin.

## Layout

| Path | What |
|---|---|
| `main.tscn`, `scripts/` | Game client (`main.gd`, `entity_node.gd`, `ui/…`). |
| `spacetime_bindings/schema/` | Generated bindings for the `blackholio` module. |
| `addons/SpacetimeDB/` | Vendored SpacetimeDB Godot SDK (editor plugin + runtime). |
| `shaders/`, `icons/` | Food shader, cursor art. |

## Run a server to test against

You need a local SpacetimeDB running a Blackholio module named `blackholio`.
Use either server module in this demo — for example the Rust one:

1. **Install the SpacetimeDB CLI** (verified with **2.5.0**):
   <https://spacetimedb.com/install>

2. **Start a local server** (keep it running):
   ```sh
   spacetime start --data-dir ~/.local/share/spacetime-blackholio
   ```

3. **Publish the module** as `blackholio`:
   ```sh
   spacetime publish -s local blackholio -p ../server-rust --delete-data -y
   ```

## Run the client

Open this folder in **Godot 4.7** and press **F5**. Enter a name to spawn.

Controls: mouse to move, **Space** to split, **S** to suicide, **Q** to lock aim.

The client connects to `http://127.0.0.1:3000`, module `blackholio` (see
`scripts/main.gd`, which calls `SpacetimeDB.Blackholio.connect_db(...)` on the
generated module autoload).

## Regenerating bindings

Only needed if the server schema changes. Use the editor SpacetimeDB dock
(configure the module URL/name once), then regenerate; or run headless:

```sh
godot --headless --path . --script res://addons/SpacetimeDB/cli.gd
```

## License

The vendored SDK addon under `addons/SpacetimeDB/` is MIT-licensed (see
`addons/SpacetimeDB/LICENSE`). The rest of this demo follows the repository
license.
