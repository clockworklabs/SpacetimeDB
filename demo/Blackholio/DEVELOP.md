Please make changes and submit pull requests to the root repository at [SpacetimeDB/demo/Blackholio](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio). The [Blackholio repository](https://github.com/clockworklabs/Blackholio) is only a mirror.

## Using local C# bindings

To build `server-csharp` against the C# bindings in this repository instead of the published packages, run this from the repository root:

```bash
dotnet nuget locals all --clear
dotnet pack -c Release crates/bindings-csharp/BSATN.Runtime
dotnet pack -c Release crates/bindings-csharp/Runtime
cargo regen csharp nuget-config
```

The last command writes a `NuGet.Config` at the repository root that points every C# project in the repository, including `server-csharp`, at the local packages. Clearing the NuGet caches first matters because NuGet reuses a cached package that has the same version number.
