#!/usr/bin/env bash

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."

cargo build --manifest-path "$STDB_PATH/crates/standalone/Cargo.toml"
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- generate -y -l csharp -o "$SDK_PATH/examples~/regression-tests/client/module_bindings" --project-path "$SDK_PATH/examples~/regression-tests/server"
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- generate -y -l csharp -o "$SDK_PATH/examples~/regression-tests/republishing/client/module_bindings" --project-path "$SDK_PATH/examples~/regression-tests/republishing/server-republish"
