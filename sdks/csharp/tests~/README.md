# Running tests
You can use `dotnet test` (either in this directory or in the project root directory) to run the tests.

# Using a different SpacetimeDB version
To run tests using the C# bindings in this repository, you need a `NuGet.Config` that points at their local build.

`cargo regen csharp nuget-config` writes it. Run it from the repository root after you `dotnet pack` the `BSATN.Runtime` package.

Lastly, before running `dotnet test`, you should `dotnet nuget locals all --clear` to clear out any cached packages. This ensures you're actually testing with the new package you just built.

Example, from the repository root:
```bash
$ dotnet pack crates/bindings-csharp/BSATN.Runtime
$ cargo regen csharp nuget-config
$ dotnet nuget locals all --clear
$ cd sdks/csharp && dotnet test
```

# Reconnect coverage

`ReconnectTests` uses the existing xUnit framework, serialized BSATN messages, a fake transport, and a controllable retry clock. Run it with:

```sh
dotnet test --filter FullyQualifiedName~ReconnectTests
```

For live server coverage, including a fault-injection proxy and short-lived JWT rotation, use the [reconnect test application](../examples~/reconnect/README.md).
