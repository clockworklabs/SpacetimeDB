# Migration note

We are in the process of moving from the `com.clockworklabs.spacetimedbsdk` repo to the `sdks/csharp` subdirectory of [SpacetimeDB](https://github.com/clockworklabs/SpacetimeDB). **Any new changes should be made there**. The `com.clockworklabs.spacetimedbsdk` repo will only be updated on release. Apologies in advance for any sharp edges while the migration is in progress.

# Notes for maintainers

## `SpacetimeDB.ClientApi`

To regenerate this namespace, run the `tools~/gen-client-api.sh` or the
`tools~/gen-client-api.bat` script.

## Developing against a local clone of SpacetimeDB
When developing against a local clone of SpacetimeDB, you'll need to ensure that the packages here can find an up-to-date version of the BSATN.Codegen and BSATN.Runtime packages from SpacetimeDB.

To develop against a local clone of SpacetimeDB at `../SpacetimeDB`, run the following command:

```sh
dotnet pack ../SpacetimeDB/crates/bindings-csharp/BSATN.Runtime && ./tools~/write-nuget-config.sh ../SpacetimeDB
```

This will create a (`.gitignore`d) `nuget.config` file that uses the local build of the package, instead of the package on NuGet.

You'll need to rerun this command whenever you update `BSATN.Codegen` or `BSATN.Runtime`.
