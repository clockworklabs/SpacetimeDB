# `sdk-test` *Rust* test

Exercise the functionality of the `SpacetimeDB` *SDK* API surface, modeling all the combination
of `types`, with several examples of `tables`, `indexes` and `reducers`.

Called as part of our tests to ensure the system is working as expected.

> **Note:** Also mirrored as a C# version at [`modules/sdk-test-cs`](../sdk-test-cs/), so must be kept in sync.

## How to Run

Execute the tests on `spacetimedb-sdk` at [test.rs](../../crates/sdk/tests/test.rs):

```bash
# Will run both Rust/C# modules
cargo test -p spacetimedb-sdk
# Only Rust
cargo test -p spacetimedb-sdk rust
# Only C#
cargo test -p spacetimedb-sdk csharp
```