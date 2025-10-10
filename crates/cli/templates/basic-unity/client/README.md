# SpacetimeDB Unity Client

A basic Unity client for SpacetimeDB.

## Setup

1. Create a new Unity project or open an existing one
2. Install the SpacetimeDB Unity SDK:
   - Add the SpacetimeDB SDK package to your project
   - See https://spacetimedb.com/docs/sdks/unity for installation instructions
3. Build and publish your server module
4. Generate bindings:
   ```
   spacetime generate --lang csharp --out-dir Assets/module_bindings
   ```
5. Copy the `Assets/Scripts/SpacetimeDBClient.cs` script to your Unity project
6. Attach the `SpacetimeDBClient` script to a GameObject in your scene
7. Update the HOST and DB_NAME constants in the script
8. Run your Unity project
