# Notes for maintainers

The directory `src/sdk/client_api` is generated from [the SpacetimeDB client-api-messages](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/client-api-messages).

The directory `src/lib/autogen` is generated from the SpacetimeDB `ModuleDef` definition using the `regen-typescript-moduledef` Rust program.

In order to regenerate both of these bindings, run `pnpm generate`.

Whenever the `client-api-messages` crate or the `ModuleDef` changes, you'll have to manually re-generate the definitions.

## Releases and publishing

In order to release and publish a new version of the package, update the version and run `npm publish`.
