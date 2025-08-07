# `module-test` *Rust* test

Called as part of our integration tests to ensure the system is working as expected.

> **Note:** Also mirrored as a C# version at [`modules/module-test-cs`](../module-test-cs/), so must be kept in sync.

## How to Run

Execute individual tests with `module-test` for *Rust* and `module-test-cs` for *C#*
at [standalone_integration_test](../../crates/testing/tests/standalone_integration_test.rs), or call

```bash
# Will run both Rust/C# module
cargo test -p spacetimedb-testing
# Only Rust
cargo test -p spacetimedb-testing rust
# Only C#
cargo test -p spacetimedb-testing csharp
```