#!/bin/env bash

# Script to run doctests.
# Note: if you get `cannot find type thing__TableHandle in this scope`, that
# is because you're trying to declare a table with indexes/unique constraints in a function scope (because all doctests
# are implicitly wrapped in a function scope.)
# Try wrapping your whole doctest in `# mod demo {` ... `# }` to fix this.

rustup run nightly cargo test --doc --target wasm32-unknown-unknown -Zdoctest-xcompile