This test client is used with the module:

- [`sdk-test-view-pk`](/modules/sdk-test-view-pk)

To (re-)generate the `module_bindings`, from this directory, run:

```sh
mkdir -p src/module_bindings
spacetime generate --lang rust --out-dir src/module_bindings --module-path ../../../../modules/sdk-test-view-pk
```
