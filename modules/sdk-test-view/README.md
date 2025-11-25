# `sdk-test-view` *Rust* test

This module tests that:
1. Rust client bindings are generated for views
2. You can register callbacks for views just like regular tables
3. Those callbacks are triggered when a view's dependencies are updated

## How to Run

Run tests named with `view` in the [Rust client SDK test suite](../../sdks/rust/tests/test.rs):

```sh
cargo test -p spacetimedb-sdk view
```
