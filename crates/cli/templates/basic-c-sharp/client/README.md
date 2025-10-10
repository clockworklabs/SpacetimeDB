# SpacetimeDB C# Client

A basic C# client for SpacetimeDB.

## Setup

1. Build and publish your server module
2. Generate bindings:
   ```
   spacetime generate --lang csharp --out-dir module_bindings
   ```
3. Run the client:
   ```
   dotnet run
   ```
