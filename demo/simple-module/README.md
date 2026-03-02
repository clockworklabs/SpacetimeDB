# Simple Module Swift Demo

This demo provides a minimal but polished SwiftUI client wired to a SpacetimeDB module, with both local and Maincloud (spacetimedb.com) test flows.

## Layout

- `spacetimedb/`: Rust module with `person` table and reducers:
  - `add(name)`
  - `delete_person(id)`
- `client-swift/`: SwiftUI app that connects, invokes reducers, and renders the replicated `person` table
  - Includes macOS-local setup controls:
    - `Start Local Server`
    - `Publish Module`
    - `Bootstrap Local` (starts server, publishes module, then connects)
  - Includes Maincloud test controls:
    - `Use Maincloud Preset`
    - `Publish Maincloud Module`
    - `Load CLI Token` (optional)

## Run It

1. Start SpacetimeDB:

   ```bash
   spacetime start
   ```

2. Publish the module:

   ```bash
   spacetime publish -s local -p demo/simple-module/spacetimedb simple-module-demo -c -y
   ```

3. Run the Swift client:

   ```bash
   cd demo/simple-module/client-swift
   swift run
   ```

The app defaults to:

- Server URL: `http://127.0.0.1:3000`
- Database name: `simple-module-demo`

On macOS, the app has an in-app numbered flow:

1. Step 1: `Start Local Server`
2. Step 2: `Publish Module`
3. Step 3: `Connect`
4. Add names to verify live replication
5. Delete names from the row `trash` button to verify replicated deletes

You can also click `Bootstrap Local (Recommended)` to run steps 1-3 automatically.

## Maincloud (spacetimedb.com) Test

1. Log in to SpacetimeDB CLI:

   ```bash
   spacetime login
   ```

2. Publish this module to Maincloud:

   ```bash
   spacetime publish -s maincloud -p demo/simple-module/spacetimedb simple-module-demo -c -y
   ```

3. Run the Swift client (`cd demo/simple-module/client-swift && swift run`), then in-app:

   - Step 1: `Use Maincloud Preset`
   - Step 2: `Publish Maincloud Module` (optional if already published via terminal)
   - Optional: `Load CLI Token`
   - Step 3: `Connect to Maincloud`
   - Add and delete names to verify replication on Maincloud

## Regenerate Swift Bindings

From repo root:

```bash
cargo run -p spacetimedb-cli -- generate \
  --lang swift \
  --out-dir demo/simple-module/client-swift/Sources/SimpleModuleClient/Generated \
  --module-path demo/simple-module/spacetimedb \
  --no-config
```
