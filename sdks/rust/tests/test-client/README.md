This test client is used with the module [`sdk-test`](/modules/sdk-test).

It is invoked by a majority of the SDK tests,
and is responsible for testing that 
serialization, deserialization, type dispatch, and client-side callbacks
work as expected.

To (re-)generate the `module_bindings`, from this directory, run:

```sh
mkdir -p src/module_bindings
spacetime generate --lang rust --out-dir src/module_bindings --project-path ../../../../modules/sdk-test
```
