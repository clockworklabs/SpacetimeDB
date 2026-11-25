# Quickstart client
See [SpacetimeDB](https://github.com/clockworklabs/SpacetimeDB)/modules/quickstart-chat

## Module tests (.NET 10)

From the project root, run:

```bash
dotnet test tests/Module.Tests.csproj
```

The sibling test project runs the module against an isolated in-memory datastore.
It uses `SpacetimeDB.Testing` and the matching `SpacetimeDB.Runtime` NuGet version;
no server, Rust compiler, or WASI workload is needed. Supported test platforms are
Linux x64/ARM64 (glibc 2.35 or newer) and Windows x64 (tested on Server 2022).
Tests are omitted when initializing the .NET 8 variant.

Keep test sources outside `spacetimedb/`. Use one execution flow per fixture and
dispose it with `using`; independent fixtures can run in parallel. Lifecycle
reducers are called explicitly. `spacetime publish` still builds the module as WASM.

## Regenerating bindings

To regenerate bindings: clone SpacetimeDB next to this repo, then in `sdks/csharp` of this repo:

```bash
tools~/gen-quickstart.sh
```
