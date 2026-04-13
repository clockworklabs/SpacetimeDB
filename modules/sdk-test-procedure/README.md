# `sdk-test-procedure` *Rust* test

This module tests that our client SDKs can invoke procedures and observe their return values.

It is separate from [`sdk-test`](../sdk-test) because module library support for procedures across languages is was not yet universal as of writing (pgoldman 2025-10-30), and so it was not possible to implement this module in [TypeScript](../sdk-test-ts) and [C#](../sdk-test-cs).

## How to Run

Run tests named with `procedure` in the [Rust client SDK test suite](../../sdks/rust/tests/test.rs):

```sh
cargo test -p spacetimedb-sdk procedure
```
