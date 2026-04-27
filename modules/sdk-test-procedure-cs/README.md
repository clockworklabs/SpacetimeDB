# `sdk-test-procedure-cs` *C#* test

This module tests that our client SDKs can invoke procedures and observe their return values.

It matches the functionality of the Rust [`sdk-test-procedure`](../sdk-test-procedure) module.

## How to Run

Run tests named with `procedure` in the [Rust client SDK test suite](../../sdks/rust/tests/test.rs):

```sh
cargo test -p spacetimedb-sdk procedure
```
