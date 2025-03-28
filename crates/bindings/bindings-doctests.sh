#!/bin/env bash

# Script to run doctests.
# Note: if you get `cannot find type thing__TableHandle in this scope`, that
# means you forgot to properly wrap your doctest.
# See the top comment of README.md.

set -exo pipefail

# Test doctests
rustup run nightly cargo test --doc --target wasm32-unknown-unknown -Zdoctest-xcompile
# Make sure they also work outside wasm (use the proper boilerplate)
cargo test --doc
# And look for broken links
RUSTDOCFLAGS="-D warnings" cargo doc