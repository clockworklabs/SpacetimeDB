This test client is used with two modules:

- [`sdk-test-connect-disconnect`](/modules/sdk-test-connect-disconnect)
- [`sdk-test-connect-disconnect-cs`](/modules/sdk-test-connect-disconnect-cs)

Currently, the bindings are generated using only one of those two modules,
chosen arbitrarily on each test run.
The two tests which use this client, 
`connect_disconnect_callbacks` and `connect_disconnect_callbacks_csharp`,
are not intended to test code generation.

The goal of the two tests is to verify that module-side `connect` and `disconnect` events
fire when an SDK connects or disconnects via WebSocket,
and that the client can observe mutations performed by those events.

To (re-)generate the `module_bindings`, from this directory, run:

```sh
mkdir -P src/module_bindings
spacetime generate --lang rust                                     \
    --out-dir src/module_bindings                                  \
    --project-path ../../../../modules/sdk-test-connect-disconnect
```
