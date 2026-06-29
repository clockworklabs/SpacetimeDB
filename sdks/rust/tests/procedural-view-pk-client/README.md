This test client is used with the Rust-only module:

- [`sdk-test-procedural-view-pk`](/modules/sdk-test-procedural-view-pk)

To (re-)generate the `module_bindings`, from this directory, run:

```sh
mkdir -p src/module_bindings
spacetime generate --lang rust --out-dir src/module_bindings --module-path ../../../../modules/sdk-test-procedural-view-pk
```
