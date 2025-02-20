# `sdk-test-connect-disconnect` *Rust* test

This module tests that we can observe `connect`/`disconnect` events for WebSocket connections.

> **Note:** Also mirrored as a C# version at [
`modules/sdk-test-connect-disconnect-cs`](../sdk-test-connect-disconnect-cs/),
> so must be kept in sync.

## How to Run

Execute the tests on `spacetimedb-sdk` at [test.rs](../../crates/sdk/tests/test.rs):

```bash
# Will run both Rust/C# modules
cargo test -p spacetimedb-sdk connect
```