# Swift Procedure E2E Script

`tools/swift-procedure-e2e.sh` validates the generated Swift procedure callback path against a live local SpacetimeDB instance.

It performs these steps:
- Publishes `modules/module-test` to a local database.
- Runs in-repo Swift code generation via `cargo run -p spacetimedb-cli -- generate --lang swift`.
- Compiles a temporary Swift runner against `sdks/swift` runtime sources plus the generated procedure wrapper.
- Invokes the generated procedure and waits for the callback result.

## Dependencies

- `spacetime` CLI in `PATH`
- `cargo` in `PATH`
- `swiftc` in `PATH`
- macOS (the script currently exits early on non-Darwin platforms)
- A running local SpacetimeDB server at `SERVER_URL` (default: `http://127.0.0.1:3000`)

## Environment Overrides

- `SERVER_URL`: SpacetimeDB server URL.
- `MODULE_PATH`: Module path relative to repo root (default: `modules/module-test`).
- `DB_NAME`: Database name used for publish (default: `swift-proc-e2e-<timestamp>`).
- `PROCEDURE_FILE`: Generated Swift file to compile (default: `SleepOneSecondProcedure.swift`).
- `PROCEDURE_TYPE`: Generated Swift procedure type to invoke (default: `SleepOneSecondProcedure`).

## Example

```bash
spacetime start &
tools/swift-procedure-e2e.sh
```
