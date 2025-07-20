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
