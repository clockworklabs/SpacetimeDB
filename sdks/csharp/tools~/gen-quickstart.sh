#!/usr/bin/env bash

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."

cargo build --manifest-path "$STDB_PATH/crates/standalone/Cargo.toml"
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- generate -y -l csharp -o "$STDB_PATH/templates/chat-console-cs/module_bindings" --project-path "$STDB_PATH/templates/chat-console-cs/spacetimedb"
