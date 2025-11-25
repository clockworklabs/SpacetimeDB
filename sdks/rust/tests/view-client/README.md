This test client is used with the module:

- [`sdk-test-view`](/modules/sdk-test-view)

The goal of the test is to exercise various view related
aspects of the (Rust) module ABI and the rust SDK.

To (re-)generate the `module_bindings`, from this directory, run:

```sh
mkdir -p src/module_bindings
spacetime generate --lang rust --out-dir src/module_bindings --project-path ../../../../modules/sdk-test-view
```
