#!/usr/bin/env bash

# This script requires a running local SpacetimeDB instance.

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."

# Regenerate Bindings
"$SDK_PATH/tools~/gen-regression-tests.sh"

# Build and run SpacetimeDB server
cargo build --manifest-path "$STDB_PATH/crates/standalone/Cargo.toml"

# Publish module for btree test
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish -c -y --server local -p "$SDK_PATH/examples~/regression-tests/server" btree-repro

# Publish module for republishing module test
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish -c -y --server local -p "$SDK_PATH/examples~/regression-tests/republishing/server-initial" republish-test
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" call --server local republish-test Insert 1
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish  --server local -p "$SDK_PATH/examples~/regression-tests/republishing/server-republish" --break-clients republish-test
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" call --server local republish-test Insert 2

echo "Cleanup obj~ folders generated in $SDK_PATH/examples~/regression-tests/procedure-client"
# There is a bug in the code generator that creates obj~ folders in the output directory using a Rust project.
rm -rf "$SDK_PATH/examples~/regression-tests/procedure-client"/*/obj~
rm -rf "$SDK_PATH/examples~/regression-tests/procedure-client/module_bindings"/*/obj~

# Publish module for procedure tests
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish -c -y --server local -p "$STDB_PATH/modules/sdk-test-procedure" procedure-tests


# Run client for btree test
cd "$SDK_PATH/examples~/regression-tests/client" && dotnet run -c Debug

# Run client for republishing module test
cd "$SDK_PATH/examples~/regression-tests/republishing/client" && dotnet run -c Debug

# Run client for procedure test
cd "$SDK_PATH/examples~/regression-tests/procedure-client" && dotnet run -c Debug
