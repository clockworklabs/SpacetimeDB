#!/usr/bin/env bash

# This script requires a running local SpacetimeDB instance.

set -ueo pipefail

STDB_PATH="$1"
SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"

# Regenerate Bindings
"$SDK_PATH/tools~/gen-regression-tests.sh" "$STDB_PATH"

# Build and run SpacetimeDB server
cargo build --manifest-path "$STDB_PATH/crates/standalone/Cargo.toml"

# Publish module for btree test
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish -c -y -p "$SDK_PATH/examples~/regression-tests/server" btree-repro

# Publish module for republishing module test
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish -c -y -p "$SDK_PATH/examples~/regression-tests/republishing/server-initial" republish-test
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" call republish-test Insert 1
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish -p "$SDK_PATH/examples~/regression-tests/republishing/server-republish" --break-clients republish-test
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" call republish-test Insert 2

# Run client for btree test
cd "$SDK_PATH/examples~/regression-tests/client" && dotnet run -c Debug

# Run client for republishing module test
cd "$SDK_PATH/examples~/regression-tests/republishing/client" && dotnet run -c Debug