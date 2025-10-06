#!/usr/bin/env bash

# This script requires a running local SpacetimeDB instance.

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."

"$SDK_PATH/tools~/gen-regression-tests.sh"
cargo build --manifest-path "$STDB_PATH/crates/standalone/Cargo.toml"
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish -c -y -p "$SDK_PATH/examples~/regression-tests/server" btree-repro
cd "$SDK_PATH/examples~/regression-tests/client" && dotnet run -c Debug
