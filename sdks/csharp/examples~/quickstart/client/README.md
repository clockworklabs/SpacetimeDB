# Quickstart client
See [SpacetimeDB](https://github.com/clockworklabs/SpacetimeDB)/modules/quickstart-chat

## Regenerating bindings

To regenerate bindings: clone SpacetimeDB next to this repo, then in this directory:

```bash
rm -rf module_bindings/*
pushd ../../../../SpacetimeDB
cargo run --manifest-path crates/cli/Cargo.toml -- generate --lang cs --out-dir ../com.clockworklabs.spacetimedbsdk/examples~/quickstart/client/module_bindings -p modules/quickstart-chat
popd
```