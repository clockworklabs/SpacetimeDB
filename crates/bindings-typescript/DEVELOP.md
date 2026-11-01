# Notes for maintainers

The directory `src/sdk/client_api` is generated from [the SpacetimeDB client-api-messages](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/client-api-messages).

The directory `src/lib/autogen` is generated from the SpacetimeDB `ModuleDef` definition using the `regen-typescript-moduledef` Rust program.

In order to regenerate both of these bindings, run `pnpm generate`.

Whenever the `client-api-messages` crate or the `ModuleDef` changes, you'll have to manually re-generate the definitions.

## Portable datastore Wasm

Module unit tests use a Rust datastore compiled to Wasm and bundled with the
TypeScript package. The generated Node.js loader, TypeScript declaration, and
Wasm binary are written to the ignored directory
`src/server/test-utils/portable-datastore-wasm`.

Install the Wasm target and the pinned `wasm-bindgen` CLI, then regenerate the
artifacts with:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.104 --locked
pnpm generate:test-utils-wasm
```

The generator builds `spacetimedb-portable-datastore-wasm` in release mode,
checks the `wasm-bindgen` version, removes machine-specific source paths, and
replaces the generated directory with exactly the three files consumed by the
package. Normal generation reuses `target/test-utils-wasm`, so repeated test
runs can use Cargo's compilation cache. That directory is disposable.

`pnpm test` performs cached generation before running the tests. To force a
fresh build, run `pnpm generate:test-utils-wasm --clean`. The portable datastore
CI job uses this clean mode to verify that the Rust-to-Wasm build continues to
work from scratch.

`pnpm build:package` performs a clean generation and copies the files into
`dist`, and `prepublishOnly` runs that command automatically. The
package-content check then verifies that all three files will be published.
Only the `dist` copy is included in the npm package, so package consumers do not
need Rust or `wasm-bindgen` installed.

## Releases and publishing

In order to release and publish a new version of the package, update the version and run `npm publish`.
