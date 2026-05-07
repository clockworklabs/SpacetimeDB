# Running tests
You can use `dotnet test` (either in this directory or in the project root directory) to run the tests.

# Using a different SpacetimeDB version
To run tests using a local version of the `SpacetimeDB` repo, you can add a `nuget.config` file in the **root** of this repository.

The `cargo csharp write-nuget-config` command can generate `NuGet.Config`. It takes one or more directories where `NuGet.Config` should be written. Use `--stdb-path` to point at a different SpacetimeDB repository.

Then, you need to `dotnet pack` the `BSATN.Runtime` package in the `SpacetimeDB` repo.

Lastly, before running `dotnet test`, you should `dotnet nuget locals all --clear` to clear out any cached packages. This ensures you're actually testing with the new package you just built.

Example:
```bash
$ export SPACETIMEDB_REPO_PATH="../SpacetimeDB"
$ cargo csharp write-nuget-config . --stdb-path "${SPACETIMEDB_REPO_PATH}"
$ ( cd "${SPACETIMEDB_REPO_PATH}"/crates/bindings-csharp/BSATN.Runtime && dotnet pack )
$ dotnet nuget locals all --clear
$ dotnet test
```
